# deskstat

A tray-popup dashboard for Linux (built/tested on XFCE), written in Tauri + vanilla TypeScript + Rust.

Click the tray icon, get a small popup with three tabs:

- **Stats** — Claude Code quota (5-hour + weekly), system stats (CPU/memory/disk, temps, battery, network, uptime), and VPN status (name + public IP when connected, card auto-sizes between the two states)
- **Todos** — Plane issue tracking across multiple workspaces/projects at once: filtered to issues assigned to you, done/cancelled hidden by default (toggle to show only those), inline status change, search, and a compact create-issue form
- **Power** — shutdown/suspend/hibernate on a timer

## Why a custom tray backend

Tauri's built-in tray on Linux wraps `libappindicator`, which has no separate "activate" signal from "open menu" — every click just shows the menu. `src-tauri/src/tray.rs` uses [`ksni`](https://docs.rs/ksni) instead, a pure-Rust StatusNotifierItem implementation, so a left click opens the window directly and right click still shows Open/Quit. The window itself closes on Escape, on a real click-away, or by picking the tray icon again.

## Setup

Plane integration needs one manual step — there's no settings UI yet, so config lives at:

```
~/.local/share/com.raze.deskstat/plane_settings.json
```

```json
{
  "base_url": "https://your-plane-instance",
  "api_token": "plane_api_...",
  "projects": [
    { "label": "my-project", "workspace_slug": "my-workspace", "project_id": "uuid-from-the-project-url" }
  ]
}
```

Get the API token from your Plane account's **Personal Access Tokens** page (shown once at creation — copy it then). `workspace_slug` and `project_id` come straight out of the project's URL: `.../<workspace_slug>/projects/<project_id>/...`.

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Produces a standalone binary plus `.deb`/`.rpm`/`.AppImage` bundles under `src-tauri/target/release/`. Always build through the Tauri CLI (`tauri dev` / `tauri build`), not a bare `cargo build` — the plain cargo build skips whatever tells the binary to embed the frontend instead of reaching for the dev server, so the release binary would just show a blank "connection refused" page.

## Stack

Tauri 2 · TypeScript (no framework) · Rust · [ksni](https://docs.rs/ksni) for the tray · [sysinfo](https://docs.rs/sysinfo) for system stats
