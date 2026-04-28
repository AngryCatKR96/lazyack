use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

const DEFAULT_SOCKET: &str = "/tmp/cmux.sock";

pub fn socket_path() -> String {
    env::var("CMUX_SOCKET_PATH").unwrap_or_else(|_| DEFAULT_SOCKET.to_string())
}

pub struct Client {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Client {
    pub fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let writer = stream.try_clone()?;
        let reader = BufReader::new(stream);
        Ok(Self { writer, reader })
    }

    pub fn call(&mut self, method: &str, params: Value) -> std::io::Result<Value> {
        let req = json!({ "id": "1", "method": method, "params": params });
        writeln!(&mut self.writer, "{req}")?;
        let mut resp = String::new();
        self.reader.read_line(&mut resp)?;
        serde_json::from_str(&resp)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

pub struct WaitingSurface {
    pub surface_id: String,
    pub body: String,
    pub workspace_id: String,
}

pub fn find_waiting_surface(client: &mut Client) -> std::io::Result<Option<WaitingSurface>> {
    let resp = client.call("notification.list", json!({}))?;
    let Some(notifications) = resp["result"]["notifications"].as_array() else {
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
            return Ok(Some(WaitingSurface {
                surface_id: surface_id.to_string(),
                body: body.to_string(),
                workspace_id: n["workspace_id"].as_str().unwrap_or("").to_string(),
            }));
        }
    }
    Ok(None)
}

pub fn is_waiting(body: &str) -> bool {
    body.contains("waiting for your input") || body.contains("needs your permission")
}

pub fn inject_text(client: &mut Client, surface_id: &str, text: &str) -> std::io::Result<()> {
    let resp = client.call(
        "surface.send_text",
        json!({ "surface_id": surface_id, "text": text }),
    )?;
    if resp["ok"].as_bool() != Some(true) {
        return Err(std::io::Error::other(format!("cmux response: {resp}")));
    }
    Ok(())
}
