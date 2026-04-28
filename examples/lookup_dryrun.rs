use lazyack::cmux::{self, BodyKind};

fn main() -> std::io::Result<()> {
    let socket = cmux::socket_path();
    let mut client = cmux::Client::connect(&socket)?;

    let candidates = cmux::find_waiting_surfaces(&mut client)?;
    println!("waiting candidates: {}", candidates.len());

    if candidates.is_empty() {
        println!("(no waiting agent via notification.list — checking pane scan fallback)\n");
        match cmux::scan_panes_for_menu(&mut client)? {
            Some(s) => println!(
                "[scan path] found menu on surface {}\n  body: {}",
                s.surface_id, s.body
            ),
            None => println!("[scan path] no terminal surface currently shows a numbered menu"),
        }
        return Ok(());
    }

    let mut menu = 0;
    let mut free = 0;
    let mut unknown = 0;
    for (i, c) in candidates.iter().enumerate() {
        match c.kind {
            BodyKind::Menu => menu += 1,
            BodyKind::FreeText => free += 1,
            BodyKind::Unknown => unknown += 1,
        }
        let preview = c.body.chars().take(70).collect::<String>();
        println!(
            "  [{i}] {:<9} {}  body={}",
            format!("{:?}", c.kind),
            c.surface_id,
            preview
        );
    }
    println!("\n  totals: {menu} menu / {free} free-text / {unknown} unknown");

    println!("\n--- routing simulation ---");
    println!("digit hotkey (1/2/3):");
    if let Some(t) = candidates
        .iter()
        .find(|c| c.kind == BodyKind::Menu)
        .or_else(|| candidates.iter().find(|c| c.kind == BodyKind::Unknown))
    {
        println!("  -> {} ({:?})", t.surface_id, t.kind);
        if t.kind == BodyKind::Unknown {
            println!("     (would verify with read_text + pane pattern check)");
        }
    } else {
        println!("  -> SKIP (only FreeText candidates; digit would land in user prompt)");
    }

    println!("\nnon-digit hotkey (e.g. \"y\\n\"):");
    if let Some(t) = candidates.first() {
        println!("  -> {} ({:?})", t.surface_id, t.kind);
    }

    println!("\n--- pane scan (also runs as a fallback when no notification waits) ---");
    match cmux::scan_panes_for_menu(&mut client)? {
        Some(s) => println!(
            "  scan would have routed to: {} (kind={:?})",
            s.surface_id, s.kind
        ),
        None => println!("  no terminal surface currently shows a numbered menu"),
    }

    Ok(())
}
