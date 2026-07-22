use serde::Serialize;
use serde_json::Value;
use std::fs;

#[derive(Serialize, Clone, Default)]
pub struct ClaudeQuota {
    pub five_hour_pct: Option<u32>,
    pub five_hour_resets_at: Option<String>,
    pub weekly_pct: Option<u32>,
    pub weekly_resets_at: Option<String>,
}

// Reads Claude Code's own local usage cache — the same data the CLI itself
// shows for session/weekly limits. Refreshed by the CLI as it runs, so this
// is real quota (rolling 5h + weekly window), not a token count we derive.
#[tauri::command]
pub fn get_claude_quota() -> ClaudeQuota {
    let Some(home) = dirs::home_dir() else {
        return ClaudeQuota::default();
    };
    let path = home.join(".claude").join("usage-cache.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return ClaudeQuota::default();
    };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
        return ClaudeQuota::default();
    };

    let data = v.get("data");
    let five_hour = data.and_then(|d| d.get("five_hour"));
    let seven_day = data.and_then(|d| d.get("seven_day"));

    ClaudeQuota {
        five_hour_pct: five_hour.and_then(|f| f.get("utilization")).and_then(Value::as_u64).map(|n| n as u32),
        five_hour_resets_at: five_hour.and_then(|f| f.get("resets_at")).and_then(Value::as_str).map(str::to_string),
        weekly_pct: seven_day.and_then(|f| f.get("utilization")).and_then(Value::as_u64).map(|n| n as u32),
        weekly_resets_at: seven_day.and_then(|f| f.get("resets_at")).and_then(Value::as_str).map(str::to_string),
    }
}
