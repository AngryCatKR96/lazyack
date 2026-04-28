use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;

use serde_json::{Value, json};

fn main() -> std::io::Result<()> {
    println!("=== Probing cmux ===\n");

    cli_identify();

    let socket_path =
        env::var("CMUX_SOCKET_PATH").unwrap_or_else(|_| "/tmp/cmux.sock".to_string());
    println!("--- via Unix socket {socket_path} ---");

    let stream = UnixStream::connect(&socket_path)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let surfaces = call(&mut writer, &mut reader, "1", "surface.list", json!({}));

    let target_ref = surfaces
        .as_ref()
        .and_then(|v| v["result"]["surfaces"].as_array())
        .and_then(|arr| arr.iter().find(|s| s["focused"] != json!(true)))
        .and_then(|s| s["ref"].as_str())
        .map(String::from)
        .unwrap_or_else(|| "surface:1".to_string());
    println!("\n[probe] drilling into surface_ref = {target_ref}\n");

    let probes: Vec<(&str, &str, Value)> = vec![
        ("2", "surface.get", json!({ "surface_ref": &target_ref })),
        ("3", "notification.list", json!({})),
        ("4", "notification.list_unread", json!({})),
        ("5", "notify.list", json!({})),
        ("6", "workspace.list", json!({})),
        ("7", "pane.list", json!({})),
        ("8", "surface.capture", json!({ "surface_ref": &target_ref })),
        ("9", "surface.get_text", json!({ "surface_ref": &target_ref })),
        ("10", "surface.snapshot", json!({ "surface_ref": &target_ref })),
        ("11", "surface.read", json!({ "surface_ref": &target_ref })),
    ];

    for (id, method, params) in probes {
        call(&mut writer, &mut reader, id, method, params);
    }

    Ok(())
}

fn cli_identify() {
    println!("--- via `cmux identify --json` ---");
    match Command::new("cmux").args(["identify", "--json"]).output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            match serde_json::from_str::<Value>(&s) {
                Ok(v) => println!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
                Err(_) => println!("{s}\n"),
            }
        }
        Ok(out) => println!(
            "CLI exit {}: {}\n",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => println!("`cmux` not found: {e}\n"),
    }
}

fn call(
    writer: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    id: &str,
    method: &str,
    params: Value,
) -> Option<Value> {
    let req = json!({ "id": id, "method": method, "params": params });
    println!("--> {method}  params={params}");

    writeln!(writer, "{req}").ok()?;

    let mut resp = String::new();
    match reader.read_line(&mut resp) {
        Ok(0) => {
            println!("<-- (connection closed)\n");
            None
        }
        Ok(_) => match serde_json::from_str::<Value>(&resp) {
            Ok(v) => {
                println!(
                    "<-- {}\n",
                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| resp.clone())
                );
                Some(v)
            }
            Err(_) => {
                println!("<-- (non-json) {}\n", resp.trim());
                None
            }
        },
        Err(e) => {
            println!("read error: {e}\n");
            None
        }
    }
}
