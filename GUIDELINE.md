# Dev guideline

Notes for working on this codebase — mostly things that cost real time to
figure out once, written down so they don't cost it twice.

## Scripts

```
scripts/dev.sh          # vite + cargo watch, hot reload
scripts/build-quick.sh   # real binary, skips .deb/.rpm/.AppImage bundling — use while iterating
scripts/build.sh         # full production build with bundles
scripts/restart.sh       # kill any running instance, launch the current release binary
```

`cargo` isn't always on `PATH` in non-login shells — the scripts export
`$HOME/.cargo/bin` themselves, but if you're running cargo commands by hand,
do the same.

## Always build through the Tauri CLI

`npx tauri dev` / `npx tauri build` (or the scripts above) — never a bare
`cargo build`. Tauri's dev-vs-embed decision isn't just `cfg(debug_assertions)`;
plain `cargo build`/`cargo build --release` skip whatever context only the
Tauri CLI sets up, so the binary keeps trying to load `devUrl`
(`localhost:1420`) instead of the embedded frontend — in *either* profile.
It compiles fine and the process stays alive, so this fails silently: only
actually looking at the rendered window catches it, not a clean build log or
`ps`.

## Testing the UI

The window is created hidden (`visible: false` in `tauri.conf.json`,
`src-tauri/src/tray.rs` shows it via the tray). To see it while developing:

1. Flip `"visible": false` → `true` in `src-tauri/tauri.conf.json`
2. `scripts/build-quick.sh && scripts/restart.sh`
3. Find the window: `wmctrl -l | grep deskstat`, then
   `xwininfo -id <id>` for its position
4. `import -window <id> out.png` (ImageMagick) to screenshot it
5. **Revert `visible` to `false` and rebuild before calling anything done** —
   shipping it `true` means the window pops up unprompted on every launch.

The window sometimes fails to map (`wmctrl -l` shows nothing, `xwininfo`
reports `IsUnMapped`) even though the process is alive — a GTK/X11 timing
quirk with this window's flags (`decorations: false`, `alwaysOnTop: true`,
`skipTaskbar: true`), not an app bug. Kill it and relaunch once; if it maps
this time, move on rather than debugging further.

You cannot simulate a real tray-icon click with `xdotool` or similar — test
that by hand.

## The tray (`src-tauri/src/tray.rs`)

Uses [`ksni`](https://docs.rs/ksni) instead of Tauri's built-in
`tauri::tray::TrayIconBuilder`. The built-in one wraps `libappindicator` on
Linux, and that backend has *no click-handling code at all* — checked by
reading `tray-icon` crate's own `platform_impl/gtk/mod.rs` source directly;
`show_menu_on_left_click`/`on_tray_icon_event` are real APIs but only
implemented for macOS/Windows in that crate. So every click just opens the
attached menu, unconditionally, with no way around it from this side.

ksni implements StatusNotifierItem directly and has a real `activate()`
(primary click) separate from the menu. A few things about it that aren't
obvious from the docs:

- `icon_pixmap` wants **ARGB32, network (big-endian) byte order** — Tauri's
  `Image::rgba()` gives RGBA8, so each pixel needs reordering
  (`rgba_to_argb` in tray.rs), not just a format label swap.
- `TrayService::spawn()` uses a plain `std::thread::spawn` with the sync
  `dbus` crate — no async runtime involved, don't overthink wiring it into
  Tokio.
- Some hosts treat a tray icon with no `id()` unreliably — always set one.

## Window auto-close (Escape / click-away)

`src/main.ts` boot section. A bare focus-blur listener was tried once before
and reverted — GTK fires a spurious focus-lost event right as the window is
still realizing, hiding it before it's ever seen. Fixed by having the Rust
side emit a `window-shown` event right after `show()`/`set_focus()`, and
having the frontend ignore blur for ~400ms after that timestamp.

Separately: a native `<select>`'s open dropdown is its own top-level GTK
window, so opening one also fires a window-blur — same failure mode, wrong
cause. Guarded with a `focusin`/`focusout` (bubbling, unlike `focus`/`blur`)
listener on `document` that suppresses the blur-hide while any `<select>`
has DOM focus.

## Window height

No dynamic content-measuring — just two fixed constants in `src/main.ts`
(`HEIGHT_VPN_OFF`/`HEIGHT_VPN_ON`), swapped based on whether the VPN card is
showing its expanded (connected) or collapsed (disconnected) state. If you
add/remove content in the Stats tab, these need re-measuring: temporarily
set a tall placeholder height, screenshot, find where the content actually
ends, set the real numbers back.

## Plane API (`src-tauri/src/commands/plane.rs`)

- Personal Access Tokens are **account-level**, not workspace-scoped —
  don't assume a 403 means "wrong workspace token." A 403 body of
  `"Given API token is not valid"` means the token itself is wrong/stale, a
  membership/permission 403 reads differently.
- `state` on an issue is a UUID, not a name — resolve it against the
  project's own `/states/` endpoint (`group` field on a state is
  `backlog`/`unstarted`/`started`/`completed`/`cancelled`, that's what
  drives the done/cancelled filtering).
- Assignee filtering isn't a query param (inconsistent across self-hosted
  versions) — fetch `/users/me/` once for your own id, then filter each
  page's `assignees` array client-side (well, server-side in Rust, but
  post-fetch either way).
