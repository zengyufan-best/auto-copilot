use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use enigo::{
    Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
    Button as EnigoButton,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "mouse_click")]
    MouseClick { x: i32, y: i32, button: String },
    #[serde(rename = "mouse_move")]
    MouseMove { x: i32, y: i32 },
    #[serde(rename = "key_press")]
    KeyPress { key: String, modifiers: Vec<String> },
    #[serde(rename = "key_type")]
    KeyType { text: String },
    #[serde(rename = "delay")]
    Delay { ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub name: String,
    pub actions: Vec<Action>,
    #[serde(rename = "loopCount")]
    pub loop_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatus {
    pub running: bool,
    #[serde(rename = "currentLoop")]
    pub current_loop: u32,
    #[serde(rename = "currentAction")]
    pub current_action: u32,
}

fn parse_button(s: &str) -> EnigoButton {
    match s {
        "right" => EnigoButton::Right,
        "middle" => EnigoButton::Middle,
        _ => EnigoButton::Left,
    }
}

fn parse_key(s: &str) -> Key {
    match s.to_lowercase().as_str() {
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "escape" | "esc" => Key::Escape,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        other => {
            if let Some(c) = other.chars().next() {
                Key::Unicode(c)
            } else {
                Key::Space
            }
        }
    }
}

fn parse_modifier(s: &str) -> Key {
    match s.to_lowercase().as_str() {
        "ctrl" | "control" => Key::Control,
        "shift" => Key::Shift,
        "alt" => Key::Alt,
        "meta" | "super" | "win" | "cmd" | "command" => Key::Meta,
        _ => Key::Control,
    }
}

pub fn execute_pipeline<F>(
    pipeline: &Pipeline,
    stop_flag: Arc<AtomicBool>,
    on_status: F,
) where
    F: Fn(ExecutionStatus),
{
    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    let loops = if pipeline.loop_count == 0 { u32::MAX } else { pipeline.loop_count };

    for loop_idx in 0..loops {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        for (action_idx, action) in pipeline.actions.iter().enumerate() {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            on_status(ExecutionStatus {
                running: true,
                current_loop: loop_idx,
                current_action: action_idx as u32,
            });

            match action {
                Action::MouseClick { x, y, button } => {
                    let _ = enigo.move_mouse(*x, *y, Coordinate::Abs);
                    thread::sleep(Duration::from_millis(50));
                    let btn = parse_button(button);
                    let _ = enigo.button(btn, Direction::Click);
                }
                Action::MouseMove { x, y } => {
                    let _ = enigo.move_mouse(*x, *y, Coordinate::Abs);
                }
                Action::KeyPress { key, modifiers } => {
                    for m in modifiers {
                        let _ = enigo.key(parse_modifier(m), Direction::Press);
                    }
                    let _ = enigo.key(parse_key(key), Direction::Click);
                    for m in modifiers.iter().rev() {
                        let _ = enigo.key(parse_modifier(m), Direction::Release);
                    }
                }
                Action::KeyType { text } => {
                    let _ = enigo.text(text);
                }
                Action::Delay { ms } => {
                    let total = *ms;
                    let mut elapsed = 0u64;
                    while elapsed < total {
                        if stop_flag.load(Ordering::Relaxed) {
                            return;
                        }
                        let sleep_time = std::cmp::min(100, total - elapsed);
                        thread::sleep(Duration::from_millis(sleep_time));
                        elapsed += sleep_time;
                    }
                }
            }
        }
    }

    on_status(ExecutionStatus {
        running: false,
        current_loop: 0,
        current_action: 0,
    });
}
