use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
mod macos_perm {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }
    pub fn has_access() -> bool {
        unsafe { CGPreflightListenEventAccess() }
    }
    pub fn request_access() -> bool {
        unsafe { CGRequestListenEventAccess() }
    }
}

pub fn log(msg: &str) {
    use std::io::Write;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/auto-pilot-rec.log")
    {
        let _ = writeln!(f, "{} {}", ts, msg);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)] // KeyPress is only constructed on the rdev (non-macOS) path
pub enum RecordedAction {
    #[serde(rename = "mouse_click")]
    MouseClick { x: i32, y: i32, button: String },
    #[serde(rename = "key_press")]
    KeyPress { key: String, modifiers: Vec<String> },
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordedEvent {
    pub action: RecordedAction,
    #[serde(rename = "gapMs")]
    pub gap_ms: u64,
}

// Used by the rdev path (Windows/Linux) and the unit tests.
#[allow(dead_code)]
fn modifier_of(name: &str) -> Option<&'static str> {
    match name {
        "ControlLeft" | "ControlRight" => Some("Ctrl"),
        "ShiftLeft" | "ShiftRight" => Some("Shift"),
        "Alt" | "AltGr" => Some("Alt"),
        "MetaLeft" | "MetaRight" => Some("Meta"),
        _ => None,
    }
}

// Used by the rdev path (Windows/Linux) and the unit tests.
#[allow(dead_code)]
fn map_key(name: &str) -> Option<String> {
    // letters: "KeyA".."KeyZ"
    if let Some(rest) = name.strip_prefix("Key") {
        if rest.len() == 1 && rest.chars().next().unwrap().is_ascii_alphabetic() {
            return Some(rest.to_lowercase());
        }
    }
    // digits: "Num0".."Num9"
    if let Some(rest) = name.strip_prefix("Num") {
        if rest.len() == 1 && rest.chars().next().unwrap().is_ascii_digit() {
            return Some(rest.to_string());
        }
    }
    // function keys: "F1".."F12"
    if name.len() > 1 && name.starts_with('F') && name[1..].chars().all(|c| c.is_ascii_digit()) {
        return Some(name.to_lowercase());
    }

    let mapped = match name {
        "Return" | "KpReturn" => "enter",
        "Space" => "space",
        "Tab" => "tab",
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Escape" => "escape",
        "UpArrow" => "up",
        "DownArrow" => "down",
        "LeftArrow" => "left",
        "RightArrow" => "right",
        "Home" => "home",
        "End" => "end",
        "PageUp" => "pageup",
        "PageDown" => "pagedown",
        "BackQuote" => "`",
        "Minus" => "-",
        "Equal" => "=",
        "LeftBracket" => "[",
        "RightBracket" => "]",
        "BackSlash" => "\\",
        "SemiColon" => ";",
        "Quote" => "'",
        "Comma" => ",",
        "Dot" => ".",
        "Slash" => "/",
        _ => return None,
    };
    Some(mapped.to_string())
}

pub fn start_recording(app: AppHandle, flag: Arc<AtomicBool>) {
    log("start_recording called");

    #[cfg(target_os = "macos")]
    {
        if !macos_perm::has_access() {
            log("macos input monitoring NOT granted; requesting");
            macos_perm::request_access();
            let _ = app.emit(
                "recording-error",
                "需要「输入监控」权限才能录制。请在弹出的系统提示中点击「打开系统设置」并允许 Auto-Pilot,或前往「系统设置 → 隐私与安全性 → 输入监控」勾选 Auto-Pilot,然后完全退出并重启应用后重试。".to_string(),
            );
            flag.store(false, Ordering::Relaxed);
            return;
        }
        log("macos input monitoring granted");
    }

    // Only one global listener for the whole app lifetime; it is gated by `flag`.
    if LISTENER_STARTED.swap(true, Ordering::SeqCst) {
        log("listener already running, reusing");
        return;
    }

    #[cfg(target_os = "macos")]
    macos_tap::start(app, flag);

    #[cfg(not(target_os = "macos"))]
    rdev_listen::start(app, flag);
}

// ============================================================================
// macOS: a mouse-only CGEventTap.
//
// rdev's macOS backend resolves a unicode "name" for every key event by calling
// into the keyboard-layout APIs, which segfaults from the tap thread (the crash
// happens inside rdev before our callback runs, so catch_unwind can't help).
// We therefore never let rdev run on macOS. Instead we create our own event tap
// whose interest mask contains ONLY mouse-down events, so key events are never
// delivered to (or decoded by) us — no keyboard recording on macOS for now, but
// no crash either. Keyboard recording stays available on Windows via rdev.
// ============================================================================
#[cfg(target_os = "macos")]
mod macos_tap {
    use super::{log, RecordedAction, RecordedEvent};
    use std::os::raw::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tauri::{AppHandle, Emitter};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    type CFMachPortRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CGEventRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFAllocatorRef = *const c_void;
    type CFStringRef = *const c_void;

    type CGEventTapCallBack = extern "C" fn(
        proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        static kCFRunLoopCommonModes: CFStringRef;
    }

    const KCG_HID_EVENT_TAP: u32 = 0;
    const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

    // CGEventType values (also used as bit positions for the interest mask).
    const LEFT_MOUSE_DOWN: u32 = 1;
    const RIGHT_MOUSE_DOWN: u32 = 3;
    const OTHER_MOUSE_DOWN: u32 = 25;
    const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    struct TapContext {
        app: AppHandle,
        flag: Arc<AtomicBool>,
        last_event: Mutex<Instant>,
        tap: Mutex<CFMachPortRef>,
    }
    // The raw CFMachPortRef is only touched from the run-loop thread; the rest
    // is already Send/Sync. We promise not to share the pointer elsewhere.
    unsafe impl Send for TapContext {}
    unsafe impl Sync for TapContext {}

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef {
        let ctx = unsafe { &*(user_info as *const TapContext) };

        // The system can disable the tap; re-enable it and move on.
        if etype == TAP_DISABLED_BY_TIMEOUT || etype == TAP_DISABLED_BY_USER_INPUT {
            log("macos tap disabled by system, re-enabling");
            let tap = *ctx.tap.lock().unwrap();
            if !tap.is_null() {
                unsafe { CGEventTapEnable(tap, true) };
            }
            return event;
        }

        if !ctx.flag.load(Ordering::Relaxed) {
            return event;
        }

        let button = match etype {
            LEFT_MOUSE_DOWN => "left",
            RIGHT_MOUSE_DOWN => "right",
            OTHER_MOUSE_DOWN => "middle",
            _ => return event,
        };

        let point = unsafe { CGEventGetLocation(event) };
        let (x, y) = (point.x as i32, point.y as i32);

        let gap = {
            let mut last = ctx.last_event.lock().unwrap();
            let g = last.elapsed().as_millis() as u64;
            *last = Instant::now();
            g
        };

        log(&format!("macos mouse {} ({},{}) gap={}", button, x, y, gap));
        let _ = ctx.app.emit(
            "recorded-action",
            RecordedEvent {
                action: RecordedAction::MouseClick {
                    x,
                    y,
                    button: button.to_string(),
                },
                gap_ms: gap,
            },
        );

        event
    }

    pub fn start(app: AppHandle, flag: Arc<AtomicBool>) {
        let ctx = Box::new(TapContext {
            app,
            flag,
            last_event: Mutex::new(Instant::now()),
            tap: Mutex::new(std::ptr::null_mut()),
        });
        // Leak the context for the app lifetime; the tap callback dereferences it.
        // Move it across the thread boundary as an address (raw pointers aren't Send).
        let ctx_addr = Box::into_raw(ctx) as usize;

        std::thread::spawn(move || {
            log("macos mouse tap thread spawned");
            let ctx_ptr = ctx_addr as *mut TapContext;
            let mask: u64 = (1u64 << LEFT_MOUSE_DOWN)
                | (1u64 << RIGHT_MOUSE_DOWN)
                | (1u64 << OTHER_MOUSE_DOWN);

            let tap = unsafe {
                CGEventTapCreate(
                    KCG_HID_EVENT_TAP,
                    KCG_HEAD_INSERT_EVENT_TAP,
                    KCG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    mask,
                    tap_callback,
                    ctx_ptr as *mut c_void,
                )
            };

            if tap.is_null() {
                log("CGEventTapCreate returned null");
                let ctx = unsafe { &*ctx_ptr };
                let _ = ctx.app.emit(
                    "recording-error",
                    "无法创建鼠标监听,请确认已在「系统设置 → 隐私与安全性 → 输入监控」中授权 Auto-Pilot 后,完全退出并重启应用。".to_string(),
                );
                ctx.flag.store(false, Ordering::Relaxed);
                super::LISTENER_STARTED.store(false, Ordering::SeqCst);
                return;
            }

            {
                let ctx = unsafe { &*ctx_ptr };
                *ctx.tap.lock().unwrap() = tap;
            }

            unsafe {
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                log("macos mouse tap enabled, entering run loop");
                CFRunLoopRun();
            }
        });
    }
}

// ============================================================================
// Windows / Linux: rdev global listener (mouse + keyboard).
// ============================================================================
#[cfg(not(target_os = "macos"))]
mod rdev_listen {
    use super::{log, map_key, modifier_of, RecordedAction, RecordedEvent, LISTENER_STARTED};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    use rdev::{listen, Button, Event, EventType};
    use tauri::{AppHandle, Emitter};

    struct RecState {
        last_pos: (i32, i32),
        last_event: Instant,
        pressed: HashSet<String>,
        was_recording: bool,
    }

    pub fn start(app: AppHandle, flag: Arc<AtomicBool>) {
        let err_app = app.clone();
        thread::spawn(move || {
            log("listener thread spawned, calling rdev::listen");
            let state = RefCell::new(RecState {
                last_pos: (0, 0),
                last_event: Instant::now(),
                pressed: HashSet::new(),
                was_recording: false,
            });

            let callback = move |event: Event| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut s = state.borrow_mut();

                    if let EventType::MouseMove { x, y } = event.event_type {
                        s.last_pos = (x as i32, y as i32);
                    }

                    let recording = flag.load(Ordering::Relaxed);
                    if recording && !s.was_recording {
                        s.last_event = Instant::now();
                        s.pressed.clear();
                    }
                    s.was_recording = recording;
                    if !recording {
                        return;
                    }

                    match event.event_type {
                        EventType::ButtonPress(button) => {
                            let name = match button {
                                Button::Left => "left",
                                Button::Right => "right",
                                Button::Middle => "middle",
                                _ => return,
                            };
                            let (x, y) = s.last_pos;
                            let gap = s.last_event.elapsed().as_millis() as u64;
                            s.last_event = Instant::now();
                            let _ = app.emit(
                                "recorded-action",
                                RecordedEvent {
                                    action: RecordedAction::MouseClick {
                                        x,
                                        y,
                                        button: name.to_string(),
                                    },
                                    gap_ms: gap,
                                },
                            );
                        }
                        EventType::KeyPress(key) => {
                            let dbg = format!("{:?}", key);
                            if s.pressed.contains(&dbg) {
                                return; // ignore auto-repeat while held
                            }
                            s.pressed.insert(dbg.clone());

                            if modifier_of(&dbg).is_some() {
                                return; // modifiers attach to the next real key
                            }

                            if let Some(key_str) = map_key(&dbg) {
                                let mut modifiers: Vec<String> = Vec::new();
                                for m in ["Ctrl", "Shift", "Alt", "Meta"] {
                                    if s.pressed.iter().any(|p| modifier_of(p) == Some(m)) {
                                        modifiers.push(m.to_string());
                                    }
                                }
                                let gap = s.last_event.elapsed().as_millis() as u64;
                                s.last_event = Instant::now();
                                let _ = app.emit(
                                    "recorded-action",
                                    RecordedEvent {
                                        action: RecordedAction::KeyPress {
                                            key: key_str,
                                            modifiers,
                                        },
                                        gap_ms: gap,
                                    },
                                );
                            }
                        }
                        EventType::KeyRelease(key) => {
                            s.pressed.remove(&format!("{:?}", key));
                        }
                        _ => {}
                    }
                }));
            };

            if let Err(e) = listen(callback) {
                log(&format!("listen error: {:?}", e));
                eprintln!("[recorder] listen error: {:?}", e);
                let _ = err_app.emit(
                    "recording-error",
                    "无法监听全局输入,请在系统设置中授权 Auto-Pilot 后重试。".to_string(),
                );
                LISTENER_STARTED.store(false, Ordering::SeqCst);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_key_letters() {
        assert_eq!(map_key("KeyA").as_deref(), Some("a"));
        assert_eq!(map_key("KeyZ").as_deref(), Some("z"));
    }

    #[test]
    fn map_key_digits() {
        assert_eq!(map_key("Num0").as_deref(), Some("0"));
        assert_eq!(map_key("Num9").as_deref(), Some("9"));
    }

    #[test]
    fn map_key_function_keys() {
        assert_eq!(map_key("F1").as_deref(), Some("f1"));
        assert_eq!(map_key("F12").as_deref(), Some("f12"));
    }

    #[test]
    fn map_key_named() {
        assert_eq!(map_key("Return").as_deref(), Some("enter"));
        assert_eq!(map_key("KpReturn").as_deref(), Some("enter"));
        assert_eq!(map_key("Space").as_deref(), Some("space"));
        assert_eq!(map_key("Escape").as_deref(), Some("escape"));
        assert_eq!(map_key("UpArrow").as_deref(), Some("up"));
        assert_eq!(map_key("RightArrow").as_deref(), Some("right"));
    }

    #[test]
    fn map_key_punctuation() {
        assert_eq!(map_key("Comma").as_deref(), Some(","));
        assert_eq!(map_key("Dot").as_deref(), Some("."));
        assert_eq!(map_key("Slash").as_deref(), Some("/"));
        assert_eq!(map_key("Minus").as_deref(), Some("-"));
    }

    #[test]
    fn map_key_unknown_is_none() {
        assert_eq!(map_key("Function"), None);
        assert_eq!(map_key("Unknown(123)"), None);
        assert_eq!(map_key("KeyAB"), None);
        assert_eq!(map_key("Num42"), None);
    }

    #[test]
    fn modifier_of_known() {
        assert_eq!(modifier_of("ControlLeft"), Some("Ctrl"));
        assert_eq!(modifier_of("ControlRight"), Some("Ctrl"));
        assert_eq!(modifier_of("ShiftLeft"), Some("Shift"));
        assert_eq!(modifier_of("ShiftRight"), Some("Shift"));
        assert_eq!(modifier_of("Alt"), Some("Alt"));
        assert_eq!(modifier_of("AltGr"), Some("Alt"));
        assert_eq!(modifier_of("MetaLeft"), Some("Meta"));
        assert_eq!(modifier_of("MetaRight"), Some("Meta"));
    }

    #[test]
    fn modifier_of_non_modifier_is_none() {
        assert_eq!(modifier_of("KeyA"), None);
        assert_eq!(modifier_of("Return"), None);
    }
}
