use std::collections::HashMap;
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::HotKey,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::cmux::{self, BodyKind, WaitingSurface};
use crate::config::{Binding, Config, parse_hotkey};

pub fn run(config: Config) -> Result<(), String> {
    let event_loop = EventLoopBuilder::new().build();
    let manager =
        GlobalHotKeyManager::new().map_err(|e| format!("create hotkey manager: {e}"))?;

    let mut binding_by_id: HashMap<u32, Binding> = HashMap::new();
    for b in &config.bindings {
        let (mods, code) = parse_hotkey(&b.hotkey)
            .map_err(|e| format!("invalid hotkey '{}': {e}", b.hotkey))?;
        let hk = HotKey::new(Some(mods), code);
        manager
            .register(hk)
            .map_err(|e| format!("register '{}': {e}", b.hotkey))?;
        binding_by_id.insert(hk.id(), b.clone());
        println!("[register] {}  send={:?}", b.hotkey, b.send);
    }

    println!("\nlazyack ready. Ctrl+C to exit.\n");

    let receiver = GlobalHotKeyEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(300));
        while let Ok(event) = receiver.try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some(binding) = binding_by_id.get(&event.id) {
                handle_press(binding);
            }
        }
    });
}

fn handle_press(binding: &Binding) {
    let socket = cmux::socket_path();
    let mut client = match cmux::Client::connect(&socket) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[error] cmux connect: {e}");
            return;
        }
    };

    let candidates = match cmux::find_waiting_surfaces(&mut client) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[error] notification query: {e}");
            return;
        }
    };

    if candidates.is_empty() {
        println!("[skip] {} pressed but no waiting agent", binding.hotkey);
        return;
    }

    let is_digit = needs_menu_check(&binding.send);

    let target = pick_target(&candidates, is_digit);
    let Some(target) = target else {
        let kinds: Vec<&str> = candidates
            .iter()
            .map(|c| match c.kind {
                BodyKind::Menu => "menu",
                BodyKind::FreeText => "free_text",
                BodyKind::Unknown => "unknown",
            })
            .collect();
        println!(
            "[skip] {} requires a numbered-menu prompt; {} candidate(s) but none qualify ({:?})",
            binding.hotkey,
            candidates.len(),
            kinds
        );
        return;
    };

    let preview = target.body.chars().take(60).collect::<String>();
    println!(
        "[route] {} -> {} ({:?}) ({})",
        binding.hotkey, target.surface_id, target.kind, preview
    );

    if is_digit && target.kind == BodyKind::Unknown {
        match cmux::read_text(&mut client, &target.surface_id, 30) {
            Ok(screen) => {
                if cmux::detect_prompt_kind(&screen) != cmux::PromptKind::NumberedMenu {
                    println!(
                        "    \u{2717} skipped: body is unknown and pane shows no numbered menu"
                    );
                    return;
                }
            }
            Err(e) => {
                eprintln!("    \u{2717} skipped: read_text failed: {e}");
                return;
            }
        }
    }

    match cmux::inject_text(&mut client, &target.surface_id, &binding.send) {
        Ok(()) => println!("    \u{2713} sent"),
        Err(e) => eprintln!("    \u{2717} {e}"),
    }
}

fn pick_target<'a>(candidates: &'a [WaitingSurface], is_digit: bool) -> Option<&'a WaitingSurface> {
    if is_digit {
        // For digit hotkeys: prefer Menu first, then Unknown (verified by pane), skip FreeText.
        candidates
            .iter()
            .find(|c| c.kind == BodyKind::Menu)
            .or_else(|| candidates.iter().find(|c| c.kind == BodyKind::Unknown))
    } else {
        candidates.first()
    }
}

fn needs_menu_check(send: &str) -> bool {
    let trimmed = send.trim();
    trimmed.len() == 1 && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(kind: BodyKind, body: &str) -> WaitingSurface {
        WaitingSurface {
            surface_id: format!("S-{body}"),
            body: body.to_string(),
            workspace_id: "W".to_string(),
            kind,
        }
    }

    #[test]
    fn digit_send_requires_menu() {
        assert!(needs_menu_check("1\n"));
        assert!(needs_menu_check("2"));
    }

    #[test]
    fn arbitrary_text_does_not_require_menu() {
        assert!(!needs_menu_check("echo hello\n"));
        assert!(!needs_menu_check("y\n"));
    }

    #[test]
    fn picks_menu_over_freetext_for_digit() {
        let cands = vec![
            ws(BodyKind::FreeText, "waiting for your input"),
            ws(BodyKind::Menu, "needs your permission to use Bash"),
        ];
        let chosen = pick_target(&cands, true).unwrap();
        assert_eq!(chosen.kind, BodyKind::Menu);
    }

    #[test]
    fn returns_none_when_only_freetext_for_digit() {
        let cands = vec![ws(BodyKind::FreeText, "waiting for your input")];
        assert!(pick_target(&cands, true).is_none());
    }

    #[test]
    fn picks_first_for_non_digit() {
        let cands = vec![
            ws(BodyKind::FreeText, "waiting for your input"),
            ws(BodyKind::Menu, "needs your permission"),
        ];
        let chosen = pick_target(&cands, false).unwrap();
        assert_eq!(chosen.kind, BodyKind::FreeText);
    }
}
