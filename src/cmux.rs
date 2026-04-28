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

#[derive(Debug, Clone)]
pub struct WaitingSurface {
    pub notification_id: String,
    pub surface_id: String,
    pub body: String,
    pub workspace_id: String,
    pub kind: BodyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// Permission / numbered menu — Claude is waiting for a 1/2/3 choice.
    Menu,
    /// Empty input prompt — Claude is waiting for arbitrary text.
    FreeText,
    /// Some other notification (task summary, custom hooks, etc.).
    Unknown,
}

pub fn classify_body(body: &str) -> BodyKind {
    if body.contains("needs your permission") {
        BodyKind::Menu
    } else if body.contains("waiting for your input") {
        BodyKind::FreeText
    } else {
        BodyKind::Unknown
    }
}

pub fn find_waiting_surfaces(client: &mut Client) -> std::io::Result<Vec<WaitingSurface>> {
    let resp = client.call("notification.list", json!({}))?;
    let Some(notifications) = resp["result"]["notifications"].as_array() else {
        return Ok(vec![]);
    };
    let mut out = vec![];
    for n in notifications {
        if n["is_read"].as_bool().unwrap_or(true) {
            continue;
        }
        let body = n["body"].as_str().unwrap_or("");
        let kind = classify_body(body);
        if matches!(kind, BodyKind::Unknown) && !is_actionable(body) {
            continue;
        }
        if let Some(surface_id) = n["surface_id"].as_str() {
            out.push(WaitingSurface {
                notification_id: n["id"].as_str().unwrap_or("").to_string(),
                surface_id: surface_id.to_string(),
                body: body.to_string(),
                workspace_id: n["workspace_id"].as_str().unwrap_or("").to_string(),
                kind,
            });
        }
    }
    Ok(out)
}

fn is_actionable(body: &str) -> bool {
    body.contains("waiting") || body.contains("permission") || body.contains("approval")
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

/// Fallback: when no waiting notification exists, scan terminal surfaces
/// in the current workspace for visible numbered-menu prompts. Useful when
/// an agent is showing a menu but cmux hasn't received the OSC yet.
pub fn scan_panes_for_menu(client: &mut Client) -> std::io::Result<Option<WaitingSurface>> {
    let resp = client.call("surface.list", json!({}))?;
    let workspace_id = resp["result"]["workspace_id"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let Some(surfaces) = resp["result"]["surfaces"].as_array() else {
        return Ok(None);
    };

    for s in surfaces {
        if s["type"].as_str() != Some("terminal") {
            continue;
        }
        let Some(surface_id) = s["id"].as_str() else {
            continue;
        };
        let text = match read_text(client, surface_id, 30) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if detect_prompt_kind(&text) == PromptKind::NumberedMenu {
            return Ok(Some(WaitingSurface {
                notification_id: format!("scan:{surface_id}"),
                surface_id: surface_id.to_string(),
                body: "(detected via pane scan — no cmux notification yet)".to_string(),
                workspace_id: workspace_id.clone(),
                kind: BodyKind::Menu,
            }));
        }
    }
    Ok(None)
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
    fn classifies_permission_as_menu() {
        assert_eq!(
            classify_body("Claude needs your permission to use Bash"),
            BodyKind::Menu
        );
    }

    #[test]
    fn classifies_input_wait_as_free_text() {
        assert_eq!(
            classify_body("Claude is waiting for your input"),
            BodyKind::FreeText
        );
    }

    #[test]
    fn classifies_task_summary_as_unknown() {
        assert_eq!(
            classify_body("작업 완료. 파일 3개 수정됨."),
            BodyKind::Unknown
        );
    }

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
