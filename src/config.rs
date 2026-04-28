use std::path::PathBuf;

use global_hotkey::hotkey::{Code, Modifiers};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Binding {
    pub hotkey: String,
    pub send: String,
}

impl Config {
    pub fn defaults() -> Self {
        Self {
            bindings: vec![
                Binding {
                    hotkey: "ctrl+alt+shift+1".into(),
                    send: "1\n".into(),
                },
                Binding {
                    hotkey: "ctrl+alt+shift+2".into(),
                    send: "2\n".into(),
                },
                Binding {
                    hotkey: "ctrl+alt+shift+3".into(),
                    send: "3\n".into(),
                },
            ],
        }
    }

    pub fn load(explicit: Option<&PathBuf>) -> Result<Self, String> {
        let path = match explicit {
            Some(p) => p.clone(),
            None => default_path(),
        };
        if !path.exists() {
            return Ok(Self::defaults());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        serde_json::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))
    }
}

pub fn default_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("lazyack").join("config.json");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("lazyack")
            .join("config.json");
    }
    PathBuf::from("config.json")
}

pub fn parse_hotkey(s: &str) -> Result<(Modifiers, Code), String> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let Some((key_part, mod_parts)) = parts.split_last() else {
        return Err(format!("empty hotkey: {s}"));
    };

    let mut mods = Modifiers::empty();
    for m in mod_parts {
        match m.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" | "meta" | "win" => mods |= Modifiers::META,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "opt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            other => return Err(format!("unknown modifier: '{other}'")),
        }
    }

    let code = parse_code(key_part)?;
    Ok((mods, code))
}

fn parse_code(s: &str) -> Result<Code, String> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "f13" => Code::F13,
        "f14" => Code::F14,
        "f15" => Code::F15,
        "f16" => Code::F16,
        "f17" => Code::F17,
        "f18" => Code::F18,
        "f19" => Code::F19,
        "f20" => Code::F20,
        "enter" | "return" => Code::Enter,
        "space" => Code::Space,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "delete" | "del" => Code::Delete,
        "backspace" => Code::Backspace,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        other => return Err(format!("unknown key: '{other}'")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_combo() {
        let (m, c) = parse_hotkey("ctrl+alt+shift+1").unwrap();
        assert!(m.contains(Modifiers::CONTROL));
        assert!(m.contains(Modifiers::ALT));
        assert!(m.contains(Modifiers::SHIFT));
        assert_eq!(c, Code::Digit1);
    }

    #[test]
    fn accepts_alias_modifiers() {
        let (m, _) = parse_hotkey("cmd+option+a").unwrap();
        assert!(m.contains(Modifiers::META));
        assert!(m.contains(Modifiers::ALT));
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_hotkey("ctrl+nope").is_err());
    }

    #[test]
    fn parses_function_keys() {
        let (_, c) = parse_hotkey("f19").unwrap();
        assert_eq!(c, Code::F19);
    }
}
