use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use serde_json::{Value, json};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

const DEFAULT_SOCKET: &str = "/tmp/cmux.sock";

fn main() {
    let event_loop = EventLoopBuilder::new().build();

    let manager = GlobalHotKeyManager::new().expect("create hotkey manager");

    let mods = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
    let mut digit_by_id: HashMap<u32, &'static str> = HashMap::new();
    for (digit, code) in [("1", Code::Digit1), ("2", Code::Digit2), ("3", Code::Digit3)] {
        let hk = HotKey::new(Some(mods), code);
        manager.register(hk).expect("register hotkey");
        digit_by_id.insert(hk.id(), digit);
    }

    println!("lazyack v0.1 ready.");
    println!("Press Ctrl+Opt+Shift+1/2/3 to answer the waiting agent prompt.");
    println!("Ctrl+C to exit.");

    let receiver = GlobalHotKeyEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(300));

        while let Ok(event) = receiver.try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            let Some(&digit) = digit_by_id.get(&event.id) else {
                continue;
            };
            handle_hotkey(digit);
        }
    });
}

fn handle_hotkey(digit: &str) {
    let socket = env::var("CMUX_SOCKET_PATH").unwrap_or_else(|_| DEFAULT_SOCKET.to_string());

    match find_waiting_surface(&socket) {
        Ok(Some((surface_id, body))) => {
            let preview = body.chars().take(60).collect::<String>();
            println!("[route] '{digit}' -> {surface_id}  ({preview})");
            match inject_digit(&socket, &surface_id, digit) {
                Ok(()) => println!("    ✓ sent"),
                Err(e) => eprintln!("    ✗ inject failed: {e}"),
            }
        }
        Ok(None) => println!("[skip] '{digit}' pressed but no waiting agent"),
        Err(e) => eprintln!("[error] cmux query failed: {e}"),
    }
}

fn find_waiting_surface(socket: &str) -> std::io::Result<Option<(String, String)>> {
    let stream = UnixStream::connect(socket)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let req = json!({ "id": "q", "method": "notification.list", "params": {} });
    writeln!(writer, "{req}")?;

    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    let v: Value = serde_json::from_str(&resp)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let Some(notifications) = v["result"]["notifications"].as_array() else {
        return Ok(None);
    };

    for n in notifications {
        if n["is_read"].as_bool().unwrap_or(true) {
            continue;
        }
        let body = n["body"].as_str().unwrap_or("");
        if !is_waiting(body) {
            continue;
        }
        if let Some(surface_id) = n["surface_id"].as_str() {
            return Ok(Some((surface_id.to_string(), body.to_string())));
        }
    }
    Ok(None)
}

fn is_waiting(body: &str) -> bool {
    body.contains("waiting for your input") || body.contains("needs your permission")
}

fn inject_digit(socket: &str, surface_id: &str, digit: &str) -> std::io::Result<()> {
    let stream = UnixStream::connect(socket)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let text = format!("{digit}\n");
    let req = json!({
        "id": "send",
        "method": "surface.send_text",
        "params": { "surface_id": surface_id, "text": text }
    });
    writeln!(writer, "{req}")?;

    let mut resp = String::new();
    reader.read_line(&mut resp)?;

    let v: Value = serde_json::from_str(&resp)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if v["ok"].as_bool() != Some(true) {
        return Err(std::io::Error::other(format!("cmux response: {resp}")));
    }
    Ok(())
}
