use std::collections::HashMap;
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::HotKey,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::cmux;
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

    match cmux::find_waiting_surface(&mut client) {
        Ok(Some(target)) => {
            let preview = target.body.chars().take(60).collect::<String>();
            println!(
                "[route] {} -> {} ({})",
                binding.hotkey, target.surface_id, preview
            );

            if needs_menu_check(&binding.send) {
                match cmux::read_text(&mut client, &target.surface_id, 30) {
                    Ok(screen) => {
                        if cmux::detect_prompt_kind(&screen) != cmux::PromptKind::NumberedMenu {
                            println!(
                                "    \u{2717} skipped: surface doesn't show a numbered menu (likely free-text input)"
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
        Ok(None) => println!("[skip] {} pressed but no waiting agent", binding.hotkey),
        Err(e) => eprintln!("[error] notification query: {e}"),
    }
}

fn needs_menu_check(send: &str) -> bool {
    let trimmed = send.trim();
    trimmed.len() == 1 && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_send_requires_menu() {
        assert!(needs_menu_check("1\n"));
        assert!(needs_menu_check("2"));
        assert!(needs_menu_check(" 3 "));
    }

    #[test]
    fn arbitrary_text_does_not_require_menu() {
        assert!(!needs_menu_check("echo hello\n"));
        assert!(!needs_menu_check("y\n"));
        assert!(!needs_menu_check("12\n"));
    }
}
