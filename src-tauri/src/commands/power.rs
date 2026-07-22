use serde::Serialize;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static TIMER_ACTIVE: AtomicBool = AtomicBool::new(false);
static TIMER_ENDS_AT: AtomicU64 = AtomicU64::new(0);
static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

fn action_cell() -> &'static Mutex<String> {
    static CELL: OnceLock<Mutex<String>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(String::new()))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[derive(Serialize, Clone)]
pub struct PowerTimerStatus {
    pub active: bool,
    pub action: Option<String>,
    pub remaining_secs: i64,
}

fn system_command(action: &str) -> Option<(&'static str, Vec<&'static str>)> {
    match action {
        "shutdown" => Some(("systemctl", vec!["poweroff"])),
        "suspend" => Some(("systemctl", vec!["suspend"])),
        "hibernate" => Some(("systemctl", vec!["hibernate"])),
        _ => None,
    }
}

#[tauri::command]
pub fn start_power_timer(action: String, minutes: u32) -> Result<(), String> {
    if system_command(&action).is_none() {
        return Err(format!("unknown action: {action}"));
    }
    if minutes == 0 {
        return Err("minutes must be greater than 0".into());
    }

    let ends_at = now_secs() + (minutes as u64 * 60);
    TIMER_ENDS_AT.store(ends_at, Ordering::SeqCst);
    TIMER_ACTIVE.store(true, Ordering::SeqCst);
    CANCEL_FLAG.store(false, Ordering::SeqCst);
    *action_cell().lock().unwrap() = action.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            if CANCEL_FLAG.load(Ordering::SeqCst) {
                TIMER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
            if now_secs() >= TIMER_ENDS_AT.load(Ordering::SeqCst) {
                TIMER_ACTIVE.store(false, Ordering::SeqCst);
                if let Some((cmd, args)) = system_command(&action) {
                    let _ = Command::new(cmd).args(args).spawn();
                }
                return;
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_power_timer() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
    TIMER_ACTIVE.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn get_power_timer_status() -> PowerTimerStatus {
    let active = TIMER_ACTIVE.load(Ordering::SeqCst);
    let remaining = if active {
        (TIMER_ENDS_AT.load(Ordering::SeqCst) as i64 - now_secs() as i64).max(0)
    } else {
        0
    };
    let action = if active {
        action_cell().lock().ok().map(|s| s.clone())
    } else {
        None
    };
    PowerTimerStatus { active, action, remaining_secs: remaining }
}
