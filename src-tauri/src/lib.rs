mod commands;
mod tray;

use commands::claude_usage::get_claude_usage;
use commands::plane::{get_plane_settings, plane_create_issue, plane_fetch_issues, plane_update_issue_state, save_plane_settings};
use commands::power::{cancel_power_timer, get_power_timer_status, start_power_timer};
use commands::quota::get_claude_quota;
use commands::system::get_system_stats;
use commands::vpn::get_vpn_status;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_system_stats,
            get_claude_usage,
            get_claude_quota,
            get_plane_settings,
            save_plane_settings,
            plane_fetch_issues,
            plane_create_issue,
            plane_update_issue_state,
            start_power_timer,
            cancel_power_timer,
            get_power_timer_status,
            get_vpn_status,
        ])
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // keep the app (and tray) alive after the window is hidden
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
