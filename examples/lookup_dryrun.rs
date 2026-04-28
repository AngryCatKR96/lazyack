use lazyack::cmux;

fn main() -> std::io::Result<()> {
    let socket = cmux::socket_path();
    let mut client = cmux::Client::connect(&socket)?;

    let resp = client.call("notification.list", serde_json::json!({}))?;
    let notifications = resp["result"]["notifications"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("notifications: total={}", notifications.len());

    let unread: Vec<_> = notifications
        .iter()
        .filter(|n| !n["is_read"].as_bool().unwrap_or(true))
        .collect();
    println!("           unread={}", unread.len());

    let waiting: Vec<_> = unread
        .iter()
        .copied()
        .filter(|n| cmux::is_waiting(n["body"].as_str().unwrap_or("")))
        .collect();
    println!("          waiting={}", waiting.len());

    println!();
    let Some(first) = waiting.first() else {
        println!("(no waiting agent)");
        return Ok(());
    };
    let surface_id = first["surface_id"].as_str().unwrap_or("");
    println!("[would route to]");
    println!("  surface_id  : {surface_id}");
    println!("  body        : {}", first["body"].as_str().unwrap_or(""));

    println!();
    println!("[screen check]");
    match cmux::read_text(&mut client, surface_id, 30) {
        Ok(screen) => {
            let kind = cmux::detect_prompt_kind(&screen);
            println!("  prompt kind : {kind:?}");
            if kind == cmux::PromptKind::NumberedMenu {
                println!(
                    "  digit send  : would FIRE (menu detected, safe to inject 1/2/3)"
                );
            } else {
                println!(
                    "  digit send  : would SKIP (free-text or unknown — preventing wrong injection)"
                );
            }
            println!();
            println!("[last 12 lines of pane]");
            let lines: Vec<&str> = screen.lines().collect();
            let start = lines.len().saturating_sub(12);
            for line in &lines[start..] {
                println!("  | {line}");
            }
        }
        Err(e) => println!("  read_text failed: {e}"),
    }

    if waiting.len() > 1 {
        println!();
        println!("[other waiting candidates]");
        for w in waiting.iter().skip(1) {
            println!("  - {}", w["surface_id"].as_str().unwrap_or("?"));
        }
    }

    Ok(())
}
