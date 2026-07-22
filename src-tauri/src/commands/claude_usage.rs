use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Default)]
pub struct ClaudeUsage {
    pub today_output_tokens: u64,
    pub today_cache_read_tokens: u64,
    pub today_turns: u32,
    pub today_sessions: u32,
    pub today_cost_usd: f64,
    pub last_model: Option<String>,
}

// Mirrors caveman-stats.js MODEL_OUTPUT_PRICE_PER_M — most specific prefix first.
const MODEL_PRICE: &[(&str, f64)] = &[
    ("claude-opus-4-0", 75.00),
    ("claude-opus-4-1", 75.00),
    ("claude-opus-4-2025", 75.00),
    ("claude-opus-4", 25.00),
    ("claude-sonnet-4", 15.00),
    ("claude-haiku-4", 5.00),
    ("claude-3-5-sonnet", 15.00),
    ("claude-3-5-haiku", 4.00),
    ("claude-3-opus", 75.00),
];

fn price_for(model: &str) -> Option<f64> {
    MODEL_PRICE
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, price)| *price)
}

fn walk_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

#[tauri::command]
pub fn get_claude_usage() -> ClaudeUsage {
    let Some(home) = dirs::home_dir() else {
        return ClaudeUsage::default();
    };
    let projects_dir = home.join(".claude").join("projects");
    let mut files = Vec::new();
    walk_jsonl(&projects_dir, &mut files);

    let today_local = Local::now().date_naive();

    let mut usage = ClaudeUsage::default();
    let mut sessions_touched_today = std::collections::HashSet::new();

    for file in &files {
        let Ok(raw) = fs::read_to_string(file) else { continue };
        let mut file_touched_today = false;

        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
            if entry.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(message) = entry.get("message") else { continue };
            let Some(usage_obj) = message.get("usage") else { continue };

            let ts = entry
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc).with_timezone(&Local).date_naive());

            if ts != Some(today_local) {
                continue;
            }

            file_touched_today = true;
            let output_tokens = usage_obj.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
            let cache_read = usage_obj
                .get("cache_read_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);

            usage.today_output_tokens += output_tokens;
            usage.today_cache_read_tokens += cache_read;
            usage.today_turns += 1;

            if let Some(model) = message.get("model").and_then(Value::as_str) {
                usage.last_model = Some(model.to_string());
                if let Some(price) = price_for(model) {
                    usage.today_cost_usd += (output_tokens as f64 / 1_000_000.0) * price;
                }
            }
        }

        if file_touched_today {
            sessions_touched_today.insert(file.clone());
        }
    }

    usage.today_sessions = sessions_touched_today.len() as u32;
    usage
}
