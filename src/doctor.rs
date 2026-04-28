use std::path::Path;

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use serde_json::json;

use crate::cmux;
use crate::config::{Config, parse_hotkey};

const COMMON_CONFLICTS: &[(&str, &str)] = &[
    ("cmd+alt+1", "browser tab switching (Chrome / Safari / Firefox)"),
    ("cmd+alt+2", "browser tab switching"),
    ("cmd+alt+3", "browser tab switching"),
    ("cmd+alt+4", "browser tab switching"),
    ("cmd+alt+5", "browser tab switching"),
    ("cmd+alt+6", "browser tab switching"),
    ("cmd+alt+7", "browser tab switching"),
    ("cmd+alt+8", "browser tab switching"),
    ("cmd+alt+9", "browser tab switching (last tab)"),
    ("cmd+shift+3", "macOS screenshot (full screen)"),
    ("cmd+shift+4", "macOS screenshot (region)"),
    ("cmd+shift+5", "macOS screen recording"),
    ("cmd+space", "Spotlight"),
    ("cmd+alt+space", "Raycast (default)"),
    ("ctrl+alt+cmd+.", "cmux show/hide all windows"),
];

pub fn run(config: Config) -> Result<(), String> {
    let mut ok = 0u32;
    let mut warn = 0u32;
    let mut err = 0u32;

    println!("=== lazyack doctor ===\n");

    println!("# cmux");
    let socket = cmux::socket_path();
    match cmux::Client::connect(&socket) {
        Ok(mut client) => {
            println!("[\u{2713}] socket reachable: {socket}");
            ok += 1;

            match client.call("surface.list", json!({})) {
                Ok(v) if v["ok"].as_bool() == Some(true) => {
                    let n = v["result"]["surfaces"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    println!("[\u{2713}] surface.list -> {n} surfaces");
                    ok += 1;
                }
                Ok(v) => {
                    println!("[\u{2717}] surface.list error: {}", v["error"]);
                    err += 1;
                }
                Err(e) => {
                    println!("[\u{2717}] surface.list: {e}");
                    err += 1;
                }
            }

            match client.call("notification.list", json!({})) {
                Ok(v) if v["ok"].as_bool() == Some(true) => {
                    let total = v["result"]["notifications"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let (menu, free, unknown) = v["result"]["notifications"]
                        .as_array()
                        .map(|arr| {
                            let mut m = 0;
                            let mut f = 0;
                            let mut u = 0;
                            for n in arr {
                                if n["is_read"].as_bool().unwrap_or(true) {
                                    continue;
                                }
                                let body = n["body"].as_str().unwrap_or("");
                                match cmux::classify_body(body) {
                                    cmux::BodyKind::Menu => m += 1,
                                    cmux::BodyKind::FreeText => f += 1,
                                    cmux::BodyKind::Unknown => u += 1,
                                }
                            }
                            (m, f, u)
                        })
                        .unwrap_or((0, 0, 0));
                    println!(
                        "[\u{2713}] notification.list -> {total} total, unread: {menu} menu / {free} free-text / {unknown} other"
                    );
                    ok += 1;
                }
                Ok(v) => {
                    println!("[\u{2717}] notification.list error: {}", v["error"]);
                    err += 1;
                }
                Err(e) => {
                    println!("[\u{2717}] notification.list: {e}");
                    err += 1;
                }
            }
        }
        Err(e) => {
            println!("[\u{2717}] socket unreachable at {socket}: {e}");
            println!("    Is cmux running?");
            err += 1;
        }
    }

    println!("\n# Hotkeys ({} configured)", config.bindings.len());
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => return Err(format!("create hotkey manager: {e}")),
    };

    for b in &config.bindings {
        match parse_hotkey(&b.hotkey) {
            Ok((mods, code)) => {
                let hk = HotKey::new(Some(mods), code);
                match manager.register(hk) {
                    Ok(()) => {
                        println!("[\u{2713}] {} can be registered", b.hotkey);
                        ok += 1;
                        let _ = manager.unregister(hk);
                    }
                    Err(e) => {
                        println!("[\u{2717}] {} registration failed: {e}", b.hotkey);
                        err += 1;
                    }
                }

                let normalized = normalize(&b.hotkey);
                for (combo, desc) in COMMON_CONFLICTS {
                    if normalize(combo) == normalized {
                        println!("[!] {} commonly conflicts with: {desc}", b.hotkey);
                        warn += 1;
                    }
                }
            }
            Err(e) => {
                println!("[\u{2717}] {} parse error: {e}", b.hotkey);
                err += 1;
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let karabiner = Path::new(&home).join(".config/karabiner/karabiner.json");
        if karabiner.exists() {
            println!("\n# Karabiner-Elements");
            println!("[!] config detected at {}", karabiner.display());
            println!(
                "    Karabiner intercepts keys at IOKit HID layer, before Carbon HotKey API."
            );
            println!(
                "    If a hotkey above can be registered but never fires, a Karabiner rule"
            );
            println!(
                "    may be remapping it. Use Karabiner-EventViewer or `Quit Karabiner` to verify."
            );
            warn += 1;
        }
    }

    println!("\n=== summary: {ok} ok / {warn} warning / {err} error ===");
    if err > 0 {
        Err(format!("{err} check(s) failed"))
    } else {
        Ok(())
    }
}

fn normalize(s: &str) -> String {
    let mut tokens: Vec<&str> = s.split('+').map(str::trim).collect();
    let key = tokens.pop().unwrap_or("").to_lowercase();
    let mut mods: Vec<&str> = tokens
        .iter()
        .map(|m| match m.to_lowercase().as_str() {
            "command" | "super" | "meta" | "win" | "cmd" => "cmd",
            "control" | "ctrl" => "ctrl",
            "option" | "opt" | "alt" => "alt",
            "shift" => "shift",
            _ => "",
        })
        .filter(|s| !s.is_empty())
        .collect();
    mods.sort();
    mods.dedup();
    if mods.is_empty() {
        key
    } else {
        format!("{}+{}", mods.join("+"), key)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalize_orders_modifiers() {
        assert_eq!(normalize("shift+ctrl+alt+1"), "alt+ctrl+shift+1");
        assert_eq!(normalize("ALT+SHIFT+CTRL+1"), "alt+ctrl+shift+1");
    }

    #[test]
    fn normalize_aliases_modifiers() {
        assert_eq!(normalize("command+option+1"), normalize("cmd+alt+1"));
    }
}
