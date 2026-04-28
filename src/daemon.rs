use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::HotKey,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::cmux::{self, BodyKind, Client, WaitingSurface};
use crate::config::{Binding, Config, parse_hotkey};

/// Env var the daemon-mode parent sets when handing off an
/// already-authenticated cmux fd to the orphaned child.
const INHERITED_FD_ENV: &str = "LAZYACK_CMUX_FD";

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
    let consumed: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let client_cell: RefCell<Option<Client>> = RefCell::new(initial_client());

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(300));
        while let Ok(event) = receiver.try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some(binding) = binding_by_id.get(&event.id) {
                handle_press(binding, &consumed, &client_cell);
            }
        }
    });
}

/// Build the cmux client used for the daemon's lifetime.
/// In `lazyack run -d` the parent opens the connection while it still has a
/// valid cmux parent chain and passes the fd via `LAZYACK_CMUX_FD`. In
/// foreground mode (no env var set), connect fresh.
fn initial_client() -> Option<Client> {
    if let Ok(fd_str) = std::env::var(INHERITED_FD_ENV) {
        // SAFETY: we trust spawn_detached to set this var only when it has
        // just opened a UnixStream and cleared CLOEXEC on it. The Client
        // takes ownership of the fd from here on.
        unsafe { std::env::remove_var(INHERITED_FD_ENV) };
        match fd_str.parse::<i32>() {
            Ok(fd) => match unsafe { Client::from_raw_fd(fd) } {
                Ok(c) => return Some(c),
                Err(e) => eprintln!("[error] inherited cmux fd unusable: {e}"),
            },
            Err(e) => eprintln!("[error] {INHERITED_FD_ENV} not an int: {e}"),
        }
    }
    match Client::connect(&cmux::socket_path()) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("[error] cmux connect: {e}");
            None
        }
    }
}

fn handle_press(
    binding: &Binding,
    consumed: &RefCell<HashSet<String>>,
    client_cell: &RefCell<Option<Client>>,
) {
    let mut client_opt = client_cell.borrow_mut();
    if client_opt.is_none() {
        match Client::connect(&cmux::socket_path()) {
            Ok(c) => *client_opt = Some(c),
            Err(e) => {
                eprintln!("[error] cmux connect: {e}");
                return;
            }
        }
    }
    let client = client_opt.as_mut().expect("client just populated");

    let all = match cmux::find_waiting_surfaces(client) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[error] notification query: {e}");
            // Drop the connection so the next press tries to reconnect.
            // (In daemon mode the reconnect will fail because cmux rejects
            // orphans, but for foreground/cmux-restart cases it recovers.)
            *client_opt = None;
            return;
        }
    };

    {
        let live: HashSet<&str> = all.iter().map(|c| c.notification_id.as_str()).collect();
        consumed.borrow_mut().retain(|id| live.contains(id.as_str()));
    }

    let candidates: Vec<&WaitingSurface> = {
        let consumed_ref = consumed.borrow();
        all.iter()
            .filter(|c| !consumed_ref.contains(&c.notification_id))
            .collect()
    };

    if candidates.is_empty() {
        if all.is_empty() {
            println!("[skip] {} pressed but no waiting agent", binding.hotkey);
        } else {
            println!(
                "[skip] {} pressed but all {} waiting notification(s) already handled by lazyack",
                binding.hotkey,
                all.len()
            );
        }
        return;
    }

    let is_digit = needs_menu_check(&binding.send);

    let target = pick_target(&candidates, is_digit);
    let Some(target) = target else {
        let kinds: Vec<&str> = candidates.iter().map(|c| kind_name(c.kind)).collect();
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
        match cmux::read_text(client, &target.surface_id, 30) {
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
                *client_opt = None;
                return;
            }
        }
    }

    match cmux::inject_text(client, &target.surface_id, &binding.send) {
        Ok(()) => {
            consumed.borrow_mut().insert(target.notification_id.clone());
            println!("    \u{2713} sent");
        }
        Err(e) => {
            eprintln!("    \u{2717} {e}");
            *client_opt = None;
        }
    }
}

fn kind_name(k: BodyKind) -> &'static str {
    match k {
        BodyKind::Menu => "menu",
        BodyKind::FreeText => "free_text",
        BodyKind::Unknown => "unknown",
    }
}

fn pick_target<'a>(
    candidates: &'a [&'a WaitingSurface],
    is_digit: bool,
) -> Option<&'a WaitingSurface> {
    if is_digit {
        candidates
            .iter()
            .find(|c| c.kind == BodyKind::Menu)
            .copied()
            .or_else(|| {
                candidates
                    .iter()
                    .find(|c| c.kind == BodyKind::Unknown)
                    .copied()
            })
    } else {
        candidates.first().copied()
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
            notification_id: format!("N-{body}"),
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
        let owned = vec![
            ws(BodyKind::FreeText, "waiting for your input"),
            ws(BodyKind::Menu, "needs your permission to use Bash"),
        ];
        let cands: Vec<&WaitingSurface> = owned.iter().collect();
        let chosen = pick_target(&cands, true).unwrap();
        assert_eq!(chosen.kind, BodyKind::Menu);
    }

    #[test]
    fn returns_none_when_only_freetext_for_digit() {
        let owned = vec![ws(BodyKind::FreeText, "waiting for your input")];
        let cands: Vec<&WaitingSurface> = owned.iter().collect();
        assert!(pick_target(&cands, true).is_none());
    }

    #[test]
    fn picks_first_for_non_digit() {
        let owned = vec![
            ws(BodyKind::FreeText, "waiting for your input"),
            ws(BodyKind::Menu, "needs your permission"),
        ];
        let cands: Vec<&WaitingSurface> = owned.iter().collect();
        let chosen = pick_target(&cands, false).unwrap();
        assert_eq!(chosen.kind, BodyKind::FreeText);
    }
}
