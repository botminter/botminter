use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::config;
use crate::state;

/// Show brain member logs: stderr + LLM conversation.
///
/// Displays two sections:
/// 1. Brain stderr log (multiplexer/bridge/ACP events)
/// 2. LLM conversation entries (tool calls and text responses from JSONL)
pub fn brain_logs(
    member: &str,
    team_flag: Option<&str>,
    stderr_lines: usize,
    llm_entries: usize,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let ws = team.path.join(member);

    if !ws.is_dir() {
        bail!(
            "Workspace not found: {}\nRun `bm teams sync` to create workspaces.",
            ws.display()
        );
    }

    // Check if this member is in brain mode
    let runtime = state::load().unwrap_or_default();
    let key = format!("{}/{}", team.name, member);
    if let Some(member_state) = runtime.members.get(&key) {
        if !member_state.brain_mode {
            eprintln!(
                "Warning: {} is not in brain mode (no brain-prompt.md)",
                member
            );
        }
    }

    // Section 1: Brain stderr
    show_brain_stderr(&ws, stderr_lines);

    // Section 2: LLM conversation
    show_llm_conversation(&ws, llm_entries);

    Ok(())
}

/// Display the last N lines of brain-stderr.log.
fn show_brain_stderr(workspace: &Path, max_lines: usize) {
    let log_path = workspace.join("brain-stderr.log");

    println!("── brain stderr (last {} lines) ──", max_lines);

    if !log_path.exists() {
        println!("  (no brain-stderr.log found at {})", log_path.display());
        println!();
        return;
    }

    let file = match fs::File::open(&log_path) {
        Ok(f) => f,
        Err(e) => {
            println!("  (failed to read {}: {})", log_path.display(), e);
            println!();
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut tail: VecDeque<String> = VecDeque::with_capacity(max_lines);

    for line in reader.lines().map_while(Result::ok) {
        if tail.len() == max_lines {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    for line in &tail {
        // Strip the date prefix (keep just time + level + message)
        // Format: "2026-03-23T08:11:02.917534Z  INFO bm::..."
        if let Some(t_pos) = line.find('T') {
            if let Some(z_pos) = line[t_pos..].find('Z') {
                let time = &line[t_pos + 1..t_pos + z_pos];
                let rest = &line[t_pos + z_pos + 1..];
                println!("{} {}", &time[..time.len().min(8)], rest.trim());
                continue;
            }
        }
        println!("{}", line);
    }

    println!();
}

/// Display LLM conversation entries from Claude Code JSONL logs.
fn show_llm_conversation(workspace: &Path, max_entries: usize) {
    let project_dir = match claude_project_dir(workspace) {
        Some(dir) => dir,
        None => {
            println!("── LLM conversation ──");
            println!(
                "  (no Claude Code logs found for {})",
                workspace.display()
            );
            println!();
            return;
        }
    };

    let jsonl_path = match find_latest_jsonl(&project_dir) {
        Some(path) => path,
        None => {
            println!("── LLM conversation ──");
            println!("  (no session files in {})", project_dir.display());
            println!();
            return;
        }
    };

    let session_id = jsonl_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let short_id = &session_id[..session_id.len().min(8)];

    println!("── LLM conversation (session {}, last {} entries) ──", short_id, max_entries);

    let file = match fs::File::open(&jsonl_path) {
        Ok(f) => f,
        Err(e) => {
            println!("  (failed to read {}: {})", jsonl_path.display(), e);
            println!();
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut entries: VecDeque<String> = VecDeque::with_capacity(max_entries);

    for line in reader.lines().map_while(Result::ok) {
        let parsed = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = parsed["type"].as_str().unwrap_or("");
        let timestamp = parsed["timestamp"]
            .as_str()
            .and_then(extract_time)
            .unwrap_or_default();

        if entry_type == "assistant" {
            if let Some(content) = parsed["message"]["content"].as_array() {
                for block in content {
                    let block_type = block["type"].as_str().unwrap_or("");
                    let formatted = match block_type {
                        "tool_use" => format_tool_use(block, &timestamp),
                        "text" => format_text(block, &timestamp),
                        _ => None,
                    };
                    if let Some(line) = formatted {
                        if entries.len() == max_entries {
                            entries.pop_front();
                        }
                        entries.push_back(line);
                    }
                }
            }
        } else if entry_type == "user" {
            // Show tool results (truncated) for context
            if let Some(result) = parsed.get("toolUseResult") {
                if let Some(content) = result["content"].as_str() {
                    let truncated = truncate(content, 100);
                    let line = format!("[{}] RESULT: {}", timestamp, truncated);
                    if entries.len() == max_entries {
                        entries.pop_front();
                    }
                    entries.push_back(line);
                }
            }
        }
    }

    if entries.is_empty() {
        println!("  (no conversation entries found)");
    } else {
        for entry in &entries {
            println!("{}", entry);
        }
    }

    println!();
}

/// Format a tool_use content block for display.
fn format_tool_use(block: &serde_json::Value, timestamp: &str) -> Option<String> {
    let name = block["name"].as_str().unwrap_or("unknown");
    let input = &block["input"];

    // Strip mcp__acp__ prefix for readability
    let short_name = name
        .strip_prefix("mcp__acp__")
        .unwrap_or(name);

    // For Bash, show command and background flag
    if short_name == "Bash" {
        let cmd = input["command"].as_str().unwrap_or("?");
        let bg = input["run_in_background"].as_bool().unwrap_or(false);
        let cmd_display = truncate(cmd, 80);
        Some(format!(
            "[{}] TOOL: {} {{ cmd: \"{}\", bg: {} }}",
            timestamp, short_name, cmd_display, bg
        ))
    } else if short_name == "Read" || short_name == "Write" || short_name == "Edit" {
        let path = input["file_path"]
            .as_str()
            .or_else(|| input["path"].as_str())
            .unwrap_or("?");
        // Show just the filename
        let file_name = Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(path);
        Some(format!(
            "[{}] TOOL: {} {{ {} }}",
            timestamp, short_name, file_name
        ))
    } else if short_name == "Grep" || short_name == "Glob" {
        let pattern = input["pattern"]
            .as_str()
            .unwrap_or("?");
        Some(format!(
            "[{}] TOOL: {} {{ \"{}\" }}",
            timestamp, short_name, truncate(pattern, 40)
        ))
    } else {
        // Generic: show tool name + truncated input
        let input_str = input.to_string();
        Some(format!(
            "[{}] TOOL: {} {{ {} }}",
            timestamp, short_name, truncate(&input_str, 80)
        ))
    }
}

/// Format a text content block for display.
fn format_text(block: &serde_json::Value, timestamp: &str) -> Option<String> {
    let text = block["text"].as_str()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(format!("[{}] TEXT: {}", timestamp, truncate(text, 200)))
}

/// Find the Claude Code project directory for a workspace.
///
/// Claude Code stores conversation logs in `~/.claude/projects/{dir_name}/`
/// where `dir_name` is the absolute workspace path with `/` and `.` replaced by `-`.
fn claude_project_dir(workspace: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let ws_abs = workspace.canonicalize().ok()?;
    let dir_name = ws_abs
        .to_str()?
        .replace(['/', '.'], "-");
    let dir = home.join(".claude").join("projects").join(&dir_name);
    dir.is_dir().then_some(dir)
}

/// Find the most recently modified JSONL file in a directory.
fn find_latest_jsonl(dir: &Path) -> Option<PathBuf> {
    let mut jsonl_files: Vec<_> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "jsonl")
        })
        .collect();

    jsonl_files.sort_by_key(|e| {
        std::cmp::Reverse(
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH),
        )
    });

    jsonl_files.first().map(|e| e.path())
}

/// Extract HH:MM:SS from an ISO timestamp string.
fn extract_time(ts: &str) -> Option<String> {
    // "2026-03-23T08:11:02.917Z" → "08:11:02"
    let t_pos = ts.find('T')?;
    let time_part = &ts[t_pos + 1..];
    Some(time_part[..time_part.len().min(8)].to_string())
}

/// Truncate a string to max_len, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    // Normalize whitespace (collapse newlines/tabs to spaces)
    let normalized: String = s.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
    if normalized.len() <= max_len {
        normalized
    } else {
        let mut end = max_len;
        while end > 0 && !normalized.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}[...]", &normalized[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_project_dir_path_derivation() {
        // Verify the path transformation logic
        let path = "/home/user/.botminter/workspaces/team/member";
        let expected = path.replace(['/', '.'], "-");
        assert_eq!(
            expected,
            "-home-user--botminter-workspaces-team-member"
        );
    }

    #[test]
    fn extract_time_from_iso() {
        assert_eq!(
            extract_time("2026-03-23T08:11:02.917Z"),
            Some("08:11:02".to_string())
        );
        assert_eq!(
            extract_time("2026-03-23T23:59:59Z"),
            Some("23:59:59".to_string())
        );
        assert_eq!(extract_time("invalid"), None);
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world foo", 10), "hello worl[...]");
    }

    #[test]
    fn truncate_normalizes_whitespace() {
        assert_eq!(truncate("hello\nworld", 20), "hello world");
    }

    #[test]
    fn format_tool_use_bash() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "mcp__acp__Bash",
            "input": {
                "command": "gh issue list --json number",
                "run_in_background": true
            }
        });
        let result = format_tool_use(&block, "08:11:02").unwrap();
        assert!(result.contains("Bash"));
        assert!(result.contains("bg: true"));
        assert!(result.contains("gh issue list"));
    }

    #[test]
    fn format_tool_use_read() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "mcp__acp__Read",
            "input": {
                "file_path": "/home/user/workspace/src/main.rs"
            }
        });
        let result = format_tool_use(&block, "08:11:02").unwrap();
        assert!(result.contains("Read"));
        assert!(result.contains("main.rs"));
    }

    #[test]
    fn format_text_entry() {
        let block = serde_json::json!({
            "type": "text",
            "text": "I'll check the board for you."
        });
        let result = format_text(&block, "08:11:02").unwrap();
        assert!(result.contains("TEXT:"));
        assert!(result.contains("check the board"));
    }

    #[test]
    fn format_text_empty_returns_none() {
        let block = serde_json::json!({
            "type": "text",
            "text": "  "
        });
        assert!(format_text(&block, "08:11:02").is_none());
    }
}
