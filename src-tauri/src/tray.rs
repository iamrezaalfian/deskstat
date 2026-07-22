use tauri::{AppHandle, Emitter, Manager};

// Anchoring to a fixed corner near where the tray actually sits — the click
// event does carry a live position, but XFCE's panel is at a predictable
// spot and a fixed anchor is simpler than trusting per-click coordinates.
fn toggle_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let visible = win.is_visible().unwrap_or(false);
    if visible {
        let _ = win.hide();
        return;
    }

    let win_w = 340.0;
    let top_margin = 34.0; // clears the XFCE top panel
    let right_margin = 20.0;

    if let Ok(Some(monitor)) = win.primary_monitor() {
        let size = monitor.size();
        let pos_x = (size.width as f64 - win_w - right_margin).max(0.0);
        let _ = win.set_position(tauri::PhysicalPosition::new(pos_x, top_margin));
    }

    let _ = win.show();
    let _ = win.set_focus();
    // lets the frontend start a grace period before it'll act on a blur —
    // GTK fires a spurious focus-lost right as the window is still
    // realizing, which would otherwise hide it before it's ever seen.
    let _ = app.emit("window-shown", ());
}

// StatusNotifierItem pixmaps are ARGB32, network (big-endian) byte order —
// Tauri's Image gives RGBA8, so each pixel needs reordering, not just a
// format label change.
fn rgba_to_argb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect()
}

struct DeskstatTray {
    app: AppHandle,
}

// Tauri's built-in tray (tauri::tray::TrayIconBuilder) wraps libappindicator
// on Linux, which never implements a click/activate signal at all — see
// tray-icon crate's platform_impl/gtk/mod.rs, there's simply no code path
// for it. ksni implements the StatusNotifierItem spec directly in Rust with
// a real Activate (primary click) distinct from the context menu, so it's
// used here instead of Tauri's tray on this platform.
impl ksni::Tray for DeskstatTray {
    fn id(&self) -> String {
        "com.raze.deskstat".into()
    }

    fn title(&self) -> String {
        "deskstat".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let Some(img) = self.app.default_window_icon() else { return Vec::new() };
        vec![ksni::Icon {
            width: img.width() as i32,
            height: img.height() as i32,
            data: rgba_to_argb(img.rgba()),
        }]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        toggle_window(&self.app);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Open deskstat".into(),
                activate: Box::new(|this: &mut Self| toggle_window(&this.app)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|this: &mut Self| this.app.exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    ksni::TrayService::new(DeskstatTray { app: app.clone() }).spawn();
    Ok(())
}
