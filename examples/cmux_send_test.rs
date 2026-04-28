use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

fn main() -> std::io::Result<()> {
    let target_arg = env::args().nth(1);

    let socket_path =
        env::var("CMUX_SOCKET_PATH").unwrap_or_else(|_| "/tmp/cmux.sock".to_string());
    let stream = UnixStream::connect(&socket_path)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    println!("=== Surfaces in current workspace ===");
    let surfaces = list_surfaces(&mut writer, &mut reader);

    let Some(target_arg) = target_arg else {
        eprintln!();
        eprintln!("Usage: cargo run --example cmux_send_test -- <surface_ref or surface_id>");
        eprintln!("  surface_ref like 'surface:42' will be auto-resolved to UUID.");
        eprintln!();
        eprintln!("WARNING: pick an EMPTY SHELL surface, not a Claude session.");
        return Ok(());
    };

    let target_uuid = if target_arg.starts_with("surface:") {
        match surfaces.iter().find(|s| s.r#ref == target_arg) {
            Some(s) => s.id.clone(),
            None => {
                eprintln!("Could not find {target_arg} in current workspace surfaces.");
                eprintln!("Tip: only surfaces of the FOCUSED workspace appear in surface.list.");
                return Ok(());
            }
        }
    } else {
        target_arg.clone()
    };

    println!("\n=== Target surface_id: {target_uuid} ===");

    // Test 1: surface.send_text with surface_id
    let r1 = do_call(
        &mut writer,
        &mut reader,
        "1",
        "surface.send_text",
        json!({ "surface_id": &target_uuid, "text": "echo hello from lazyack\n" }),
    );
    check_target(&target_uuid, &r1);

    // Test 2: send "1" without enter
    let r2 = do_call(
        &mut writer,
        &mut reader,
        "2",
        "surface.send_text",
        json!({ "surface_id": &target_uuid, "text": "1" }),
    );
    check_target(&target_uuid, &r2);

    // Test 3: send_key Enter
    let r3 = do_call(
        &mut writer,
        &mut reader,
        "3",
        "surface.send_key",
        json!({ "surface_id": &target_uuid, "key": "enter" }),
    );
    check_target(&target_uuid, &r3);

    // Fallback test if all three above hit focused surface: try pane_id instead.
    if all_missed(&target_uuid, &[&r1, &r2, &r3]) {
        println!("\n--- All hit focused surface. Falling back to pane_id ---");
        if let Some(pane_id) = surfaces
            .iter()
            .find(|s| s.id == target_uuid)
            .map(|s| s.pane_id.clone())
        {
            let r4 = do_call(
                &mut writer,
                &mut reader,
                "4",
                "surface.send_text",
                json!({ "pane_id": &pane_id, "text": "echo hello via pane_id\n" }),
            );
            check_target(&target_uuid, &r4);
        }
    }

    Ok(())
}

#[derive(Debug)]
struct Surface {
    r#ref: String,
    id: String,
    pane_id: String,
    title: String,
    focused: bool,
}

fn list_surfaces(writer: &mut UnixStream, reader: &mut BufReader<UnixStream>) -> Vec<Surface> {
    let req = json!({ "id": "L", "method": "surface.list", "params": {} });
    if writeln!(writer, "{req}").is_err() {
        return vec![];
    }
    let mut resp = String::new();
    if reader.read_line(&mut resp).is_err() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(&resp) else {
        return vec![];
    };

    let mut out = vec![];
    if let Some(surfaces) = v["result"]["surfaces"].as_array() {
        for s in surfaces {
            let surface = Surface {
                r#ref: s["ref"].as_str().unwrap_or("").to_string(),
                id: s["id"].as_str().unwrap_or("").to_string(),
                pane_id: s["pane_id"].as_str().unwrap_or("").to_string(),
                title: s["title"].as_str().unwrap_or("").to_string(),
                focused: s["focused"].as_bool().unwrap_or(false),
            };
            let mark = if surface.focused { "*" } else { " " };
            println!(
                "{mark} {:<14} {}  {}",
                surface.r#ref, surface.id, surface.title
            );
            out.push(surface);
        }
    }
    out
}

fn do_call(
    writer: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    id: &str,
    method: &str,
    params: Value,
) -> Value {
    let req = json!({ "id": id, "method": method, "params": params });
    println!("\n--> {method}  {params}");
    if writeln!(writer, "{req}").is_err() {
        return Value::Null;
    }
    let mut resp = String::new();
    if reader.read_line(&mut resp).is_err() {
        return Value::Null;
    }
    let parsed: Value =
        serde_json::from_str(&resp).unwrap_or_else(|_| json!({ "raw": resp.trim() }));
    println!(
        "<-- {}",
        serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| resp.clone())
    );
    parsed
}

fn check_target(target_uuid: &str, resp: &Value) {
    let actual = resp["result"]["surface_id"].as_str().unwrap_or("");
    if actual == target_uuid {
        println!("    ✓ HIT target");
    } else if actual.is_empty() {
        println!("    ? no surface_id in response");
    } else {
        println!("    ✗ MISS — server actually hit surface_id={actual}");
    }
}

fn all_missed(target_uuid: &str, responses: &[&Value]) -> bool {
    responses.iter().all(|r| {
        r["result"]["surface_id"]
            .as_str()
            .map(|s| s != target_uuid)
            .unwrap_or(true)
    })
}
