use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

fn main() -> std::io::Result<()> {
    let socket = env::var("CMUX_SOCKET_PATH").unwrap_or_else(|_| "/tmp/cmux.sock".to_string());
    let stream = UnixStream::connect(&socket)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let req = json!({ "id": "q", "method": "notification.list", "params": {} });
    writeln!(writer, "{req}")?;
    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    let v: Value = serde_json::from_str(&resp).expect("parse");

    let notifications = v["result"]["notifications"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("notifications: total={}", notifications.len());

    let unread: Vec<&Value> = notifications
        .iter()
        .filter(|n| !n["is_read"].as_bool().unwrap_or(true))
        .collect();
    println!("           unread={}", unread.len());

    let waiting: Vec<&Value> = unread
        .iter()
        .copied()
        .filter(|n| {
            let b = n["body"].as_str().unwrap_or("");
            b.contains("waiting for your input") || b.contains("needs your permission")
        })
        .collect();
    println!("          waiting={}", waiting.len());

    println!();
    if let Some(first) = waiting.first() {
        println!("[would route to]");
        println!(
            "  surface_id  : {}",
            first["surface_id"].as_str().unwrap_or("?")
        );
        println!("  body        : {}", first["body"].as_str().unwrap_or("?"));
        println!(
            "  workspace_id: {}",
            first["workspace_id"].as_str().unwrap_or("?")
        );
    } else {
        println!("(no waiting agent — hotkey press would be a no-op)");
    }

    if waiting.len() > 1 {
        println!();
        println!("[other waiting candidates (v0.2 HUD will disambiguate)]");
        for w in waiting.iter().skip(1) {
            println!(
                "  - {}  body={}",
                w["surface_id"].as_str().unwrap_or("?"),
                w["body"].as_str().unwrap_or("?")
            );
        }
    }

    Ok(())
}
