use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

// How long after arming a one-shot pick we ignore input events, so the very
// click/keypress that armed the pick is never captured as the picked value.
const ARM_GUARD_MS: u128 = 250;

/// Shared capture state. A single global listener reads these flags and decides
/// what to do with each input event: record it, treat it as a one-shot pick, or
/// ignore it. Held behind an `Arc` in `AppState` and the listener thread.
pub struct CaptureState {
    pub recording: AtomicBool,
    pub pick_position: AtomicBool,
    pub pick_key: AtomicBool,
    armed_at: Mutex<Instant>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            recording: AtomicBool::new(false),
            pick_position: AtomicBool::new(false),
            pick_key: AtomicBool::new(false),
            armed_at: Mutex::new(Instant::now()),
        }
    }

    /// Mark "now" as the moment a one-shot pick was armed.
    pub fn arm(&self) {
        if let Ok(mut t) = self.armed_at.lock() {
            *t = Instant::now();
        }
    }

    /// True while we are still within the guard window after arming.
    fn recently_armed(&self) -> bool {
        self.armed_at
            .lock()
            .map(|t| t.elapsed().as_millis() < ARM_GUARD_MS)
            .unwrap_or(false)
    }
}

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
    // Cross-platform: %TEMP%\auto-pilot-rec.log on Windows, /tmp/... on Unix.
    let mut path = std::env::temp_dir();
    path.push("auto-pilot-rec.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
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

#[derive(Debug, Clone, Serialize)]
pub struct PickedPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)] // only emitted on the rdev (non-macOS) path
pub struct PickedKey {
    pub key: String,
    pub modifiers: Vec<String>,
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

/// Ensure the single app-lifetime global listener is running. Safe to call
/// repeatedly; only the first call actually starts a listener. Returns an error
/// string (for the UI) when the platform refuses to start one (e.g. macOS
/// permission missing).
pub fn ensure_listener(app: AppHandle, state: Arc<CaptureState>) -> Result<(), String> {
    log("ensure_listener called");

    #[cfg(target_os = "macos")]
    {
        if !macos_perm::has_access() {
            log("macos input monitoring NOT granted; requesting");
            macos_perm::request_access();
            return Err("需要「输入监控」权限。请在「系统设置 → 隐私与安全性 → 输入监控」中勾选 Auto-Pilot,然后完全退出并重启应用后重试。".to_string());
        }
        log("macos input monitoring granted");
    }

    if LISTENER_STARTED.swap(true, Ordering::SeqCst) {
        log("listener already running, reusing");
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    macos_tap::start(app, state);

    #[cfg(not(target_os = "macos"))]
    rdev_listen::start(app, state);

    Ok(())
}

// ============================================================================
// macOS: a CGEventTap capturing mouse-down AND key-down events.
//
// rdev's macOS backend segfaults from the tap thread because it resolves a
// unicode "name" for every key event via the keyboard-layout APIs (the crash
// happens inside rdev before our callback runs, so catch_unwind can't help).
// We never let rdev run on macOS. Instead we own the event tap and, for key
// events, read only the raw integer keycode (CGEventGetIntegerValueField) and
// map it ourselves — no unicode-layout resolution, so no crash. This gives us
// keyboard pick/record on macOS as well as mouse.
// ============================================================================
#[cfg(target_os = "macos")]
mod macos_tap {
    use super::{log, CaptureState, PickedKey, PickedPosition, RecordedAction, RecordedEvent};
    use std::os::raw::c_void;
    use std::sync::atomic::Ordering;
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
        // Read an integer field (e.g. the raw keyboard keycode) from an event.
        // We deliberately read ONLY the integer keycode and never ask macOS to
        // resolve a unicode key name — that name lookup (what rdev does) is what
        // segfaults on the tap thread.
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        // Modifier flags (shift/ctrl/alt/cmd) packed into a bitmask.
        fn CGEventGetFlags(event: CGEventRef) -> u64;
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
    const KEY_DOWN: u32 = 10;
    const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    // Field index for the raw keyboard keycode (kCGKeyboardEventKeycode).
    const KEYBOARD_EVENT_KEYCODE: u32 = 9;

    // CGEventFlags modifier bits.
    const FLAG_SHIFT: u64 = 1 << 17;
    const FLAG_CONTROL: u64 = 1 << 18;
    const FLAG_ALTERNATE: u64 = 1 << 19; // Option
    const FLAG_COMMAND: u64 = 1 << 20;

    fn modifiers_from_flags(flags: u64) -> Vec<String> {
        let mut mods = Vec::new();
        if flags & FLAG_CONTROL != 0 {
            mods.push("Ctrl".to_string());
        }
        if flags & FLAG_SHIFT != 0 {
            mods.push("Shift".to_string());
        }
        if flags & FLAG_ALTERNATE != 0 {
            mods.push("Alt".to_string());
        }
        if flags & FLAG_COMMAND != 0 {
            mods.push("Meta".to_string());
        }
        mods
    }

    /// Map a macOS virtual keycode to the same key-name vocabulary that the
    /// rdev path (`super::map_key`) produces, so the frontend and the executor
    /// see one consistent set of key names across platforms. Returns `None` for
    /// keys we don't handle (including pure modifier keys, which arrive as
    /// flagsChanged events, not keyDown, so they never reach here anyway).
    fn map_macos_keycode(code: i64) -> Option<String> {
        let s = match code {
            0 => "a", 1 => "s", 2 => "d", 3 => "f", 4 => "h", 5 => "g", 6 => "z",
            7 => "x", 8 => "c", 9 => "v", 11 => "b", 12 => "q", 13 => "w", 14 => "e",
            15 => "r", 16 => "y", 17 => "t", 31 => "o", 32 => "u", 34 => "i",
            35 => "p", 37 => "l", 38 => "j", 40 => "k", 45 => "n", 46 => "m",
            18 => "1", 19 => "2", 20 => "3", 21 => "4", 22 => "6", 23 => "5",
            25 => "9", 26 => "7", 28 => "8", 29 => "0",
            24 => "=", 27 => "-", 30 => "]", 33 => "[", 39 => "'", 41 => ";",
            42 => "\\", 43 => ",", 44 => "/", 47 => ".", 50 => "`",
            36 | 76 => "enter",
            48 => "tab",
            49 => "space",
            51 => "backspace",
            53 => "escape",
            115 => "home",
            116 => "pageup",
            117 => "delete",
            119 => "end",
            121 => "pagedown",
            123 => "left",
            124 => "right",
            125 => "down",
            126 => "up",
            122 => "f1", 120 => "f2", 99 => "f3", 118 => "f4", 96 => "f5",
            97 => "f6", 98 => "f7", 100 => "f8", 101 => "f9", 109 => "f10",
            103 => "f11", 111 => "f12",
            _ => return None,
        };
        Some(s.to_string())
    }

    struct TapContext {
        app: AppHandle,
        state: Arc<CaptureState>,
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

        // Keyboard: read ONLY the raw keycode integer + modifier flags. We never
        // ask macOS to resolve a unicode key name (that lookup is what crashes
        // rdev on this thread), so this is crash-safe.
        if etype == KEY_DOWN {
            let code = unsafe { CGEventGetIntegerValueField(event, KEYBOARD_EVENT_KEYCODE) };
            let key = match map_macos_keycode(code) {
                Some(k) => k,
                None => return event,
            };
            let flags = unsafe { CGEventGetFlags(event) };
            let modifiers = modifiers_from_flags(flags);

            // One-shot: pick a key.
            if ctx.state.pick_key.load(Ordering::Relaxed) {
                if ctx.state.recently_armed() {
                    return event;
                }
                ctx.state.pick_key.store(false, Ordering::Relaxed);
                log(&format!("macos picked key '{}'", key));
                let _ = ctx.app.emit("picked-key", PickedKey { key, modifiers });
                return event;
            }

            if !ctx.state.recording.load(Ordering::Relaxed) {
                return event;
            }
            let gap = {
                let mut last = ctx.last_event.lock().unwrap();
                let g = last.elapsed().as_millis() as u64;
                *last = Instant::now();
                g
            };
            log(&format!("macos key '{}' gap={}", key, gap));
            let _ = ctx.app.emit(
                "recorded-action",
                RecordedEvent {
                    action: RecordedAction::KeyPress { key, modifiers },
                    gap_ms: gap,
                },
            );
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

        // One-shot: pick a coordinate.
        if ctx.state.pick_position.load(Ordering::Relaxed) {
            if ctx.state.recently_armed() {
                return event; // ignore the click that armed the pick
            }
            ctx.state.pick_position.store(false, Ordering::Relaxed);
            log(&format!("macos picked position ({},{})", x, y));
            let _ = ctx.app.emit("picked-position", PickedPosition { x, y });
            return event;
        }

        if !ctx.state.recording.load(Ordering::Relaxed) {
            return event;
        }

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

    pub fn start(app: AppHandle, state: Arc<CaptureState>) {
        let ctx = Box::new(TapContext {
            app,
            state,
            last_event: Mutex::new(Instant::now()),
            tap: Mutex::new(std::ptr::null_mut()),
        });
        // Leak the context for the app lifetime; the tap callback dereferences it.
        // Move it across the thread boundary as an address (raw pointers aren't Send).
        let ctx_addr = Box::into_raw(ctx) as usize;

        std::thread::spawn(move || {
            log("macos input tap thread spawned");
            let ctx_ptr = ctx_addr as *mut TapContext;
            let mask: u64 = (1u64 << LEFT_MOUSE_DOWN)
                | (1u64 << RIGHT_MOUSE_DOWN)
                | (1u64 << OTHER_MOUSE_DOWN)
                | (1u64 << KEY_DOWN);

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
                    "无法创建输入监听,请确认已在「系统设置 → 隐私与安全性 → 输入监控」中授权 Auto-Pilot 后,完全退出并重启应用。".to_string(),
                );
                ctx.state.recording.store(false, Ordering::Relaxed);
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
                log("macos input tap enabled, entering run loop");
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
    use super::{
        log, map_key, modifier_of, CaptureState, PickedKey, PickedPosition, RecordedAction,
        RecordedEvent, LISTENER_STARTED,
    };
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::atomic::Ordering;
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

    fn collect_modifiers(pressed: &HashSet<String>) -> Vec<String> {
        let mut modifiers: Vec<String> = Vec::new();
        for m in ["Ctrl", "Shift", "Alt", "Meta"] {
            if pressed.iter().any(|p| modifier_of(p) == Some(m)) {
                modifiers.push(m.to_string());
            }
        }
        modifiers
    }

    pub fn start(app: AppHandle, cap: Arc<CaptureState>) {
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

                    let recording = cap.recording.load(Ordering::Relaxed);
                    let pick_position = cap.pick_position.load(Ordering::Relaxed);
                    let pick_key = cap.pick_key.load(Ordering::Relaxed);

                    // Reset the recording gap baseline each time recording begins.
                    if recording && !s.was_recording {
                        s.last_event = Instant::now();
                        s.pressed.clear();
                    }
                    s.was_recording = recording;

                    match event.event_type {
                        EventType::ButtonPress(button) => {
                            let name = match button {
                                Button::Left => "left",
                                Button::Right => "right",
                                Button::Middle => "middle",
                                _ => return,
                            };
                            let (x, y) = s.last_pos;

                            // One-shot: pick a coordinate.
                            if pick_position {
                                if cap.recently_armed() {
                                    return;
                                }
                                cap.pick_position.store(false, Ordering::Relaxed);
                                log(&format!("picked position ({},{})", x, y));
                                let _ = app.emit("picked-position", PickedPosition { x, y });
                                return;
                            }

                            if !recording {
                                return;
                            }
                            let gap = s.last_event.elapsed().as_millis() as u64;
                            s.last_event = Instant::now();
                            log(&format!("record mouse {} ({},{})", name, x, y));
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
                            // Diagnostic: always log the raw key rdev reports, so
                            // we can tell "event never arrived" apart from "arrived
                            // but the name was unmapped".
                            log(&format!("rdev keypress raw {}", dbg));

                            if s.pressed.contains(&dbg) {
                                return; // ignore auto-repeat while held
                            }
                            s.pressed.insert(dbg.clone());

                            if modifier_of(&dbg).is_some() {
                                return; // modifiers attach to the next real key
                            }

                            // Map to a usable key string, falling back to the raw
                            // lowercased name so an unmapped key still fills the
                            // pick field instead of being silently dropped.
                            let key_str = map_key(&dbg).unwrap_or_else(|| dbg.to_lowercase());
                            let modifiers = collect_modifiers(&s.pressed);

                            // One-shot: pick a key.
                            if pick_key {
                                if cap.recently_armed() {
                                    return;
                                }
                                cap.pick_key.store(false, Ordering::Relaxed);
                                log(&format!("picked key '{}'", key_str));
                                let _ = app.emit(
                                    "picked-key",
                                    PickedKey {
                                        key: key_str,
                                        modifiers,
                                    },
                                );
                                return;
                            }

                            if !recording {
                                return;
                            }
                            let gap = s.last_event.elapsed().as_millis() as u64;
                            s.last_event = Instant::now();
                            log(&format!("record key '{}'", key_str));
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
