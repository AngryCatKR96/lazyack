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

pub fn read_text(client: &mut Client, surface_id: &str, lines: u32) -> std::io::Result<String> {
    let resp = client.call(
        "surface.read_text",
        json!({ "surface_id": surface_id, "lines": lines }),
    )?;
    if resp["ok"].as_bool() != Some(true) {
        return Err(std::io::Error::other(format!("cmux response: {resp}")));
    }
    Ok(resp["result"]["text"].as_str().unwrap_or("").to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptKind {
    NumberedMenu,
    Other,
}

pub fn detect_prompt_kind(screen: &str) -> PromptKind {
    let last_lines: Vec<&str> = screen.lines().rev().take(20).collect();

    let mut menu_lines = 0;
    for line in &last_lines {
        let trimmed = line.trim_start_matches([' ', '>', '\u{276f}', '\u{203a}']);
        let trimmed = trimmed.trim_start();
        let mut chars = trimmed.chars();
        match chars.next() {
            Some(c) if c.is_ascii_digit() => {
                if let Some(next) = chars.next() {
                    if matches!(next, '.' | ')' | ']') {
                        menu_lines += 1;
                    }
                }
            }
            Some('[') => {
                if let (Some(d), Some(']')) = (chars.next(), chars.next()) {
                    if d.is_ascii_digit() {
                        menu_lines += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if menu_lines >= 2 {
        PromptKind::NumberedMenu
    } else {
        PromptKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_numbered_menu_dot() {
        let screen = "Do you want to proceed?\n  1. Yes\n  2. Yes, allow all\n  3. No";
        assert_eq!(detect_prompt_kind(screen), PromptKind::NumberedMenu);
    }

    #[test]
    fn detects_numbered_menu_bracket() {
        let screen = "[1] Yes\n[2] Yes, allow all\n[3] No";
        assert_eq!(detect_prompt_kind(screen), PromptKind::NumberedMenu);
    }

    #[test]
    fn detects_numbered_menu_with_arrow_marker() {
        let screen = "❯ 1. Yes\n  2. Yes, allow all\n  3. No";
        assert_eq!(detect_prompt_kind(screen), PromptKind::NumberedMenu);
    }

    #[test]
    fn rejects_free_text_prompt() {
        let screen = "Brewed for 1m 5s\n────────\n❯ \n────────\naccept edits on";
        assert_eq!(detect_prompt_kind(screen), PromptKind::Other);
    }

    #[test]
    fn rejects_single_numbered_line() {
        // a stray "1." in chat history shouldn't trigger menu detection
        let screen = "Earlier I said:\n  1. Some thought\nNow waiting for input.\n❯ ";
        assert_eq!(detect_prompt_kind(screen), PromptKind::Other);
    }
}
