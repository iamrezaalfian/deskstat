use serde::Serialize;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

#[derive(Serialize, Clone, Default)]
pub struct VpnStatus {
    pub connected: bool,
    pub name: Option<String>,
    pub ip: Option<String>,
}

const VPN_IFACE_PREFIXES: &[&str] = &["tun", "wg", "tailscale", "ppp", "nordlynx", "ipsec", "utun"];

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

fn iface_has_inet_addr(addrs: &[Value], iface: &str) -> bool {
    addrs
        .iter()
        .find(|entry| entry.get("ifname").and_then(Value::as_str) == Some(iface))
        .and_then(|entry| entry.get("addr_info"))
        .and_then(Value::as_array)
        .map(|infos| infos.iter().any(|a| a.get("family").and_then(Value::as_str) == Some("inet")))
        .unwrap_or(false)
}

// The IP shown is the public exit IP as seen by the internet while routed
// through the tunnel — not the tunnel interface's local address — since
// that's what "what IP does the VPN give me" actually means.
async fn fetch_public_ip() -> Option<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build().ok()?;
    let resp = client.get("https://api.ipify.org").send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let ip = text.trim();
    if ip.is_empty() {
        None
    } else {
        Some(ip.to_string())
    }
}

// Active VPN connections are found two ways: NetworkManager profiles (gives a
// friendly name) and, as a fallback, well-known VPN interface naming
// conventions (covers VPN clients that bypass NetworkManager, e.g. some
// proprietary tunnel apps).
#[tauri::command]
pub async fn get_vpn_status() -> VpnStatus {
    let Some(addr_json) = run("ip", &["-j", "addr", "show"]) else {
        return VpnStatus::default();
    };
    let Ok(addrs) = serde_json::from_str::<Vec<Value>>(&addr_json) else {
        return VpnStatus::default();
    };

    let mut connected_as: Option<(String, Option<String>)> = None;

    if let Some(nm_out) = run("nmcli", &["-t", "-f", "TYPE,DEVICE,NAME", "connection", "show", "--active"]) {
        for line in nm_out.lines() {
            let mut parts = line.splitn(3, ':');
            let (Some(conn_type), Some(device), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            if (conn_type == "vpn" || conn_type == "wireguard") && iface_has_inet_addr(&addrs, device) {
                connected_as = Some((name.to_string(), None));
                break;
            }
        }
    }

    if connected_as.is_none() {
        for entry in &addrs {
            let Some(ifname) = entry.get("ifname").and_then(Value::as_str) else { continue };
            if !VPN_IFACE_PREFIXES.iter().any(|p| ifname.starts_with(p)) {
                continue;
            }
            let operstate = entry.get("operstate").and_then(Value::as_str).unwrap_or("");
            if operstate != "UP" && operstate != "UNKNOWN" {
                continue;
            }
            if iface_has_inet_addr(&addrs, ifname) {
                connected_as = Some((ifname.to_string(), None));
                break;
            }
        }
    }

    let Some((name, _)) = connected_as else {
        return VpnStatus::default();
    };

    let ip = fetch_public_ip().await;
    VpnStatus { connected: true, name: Some(name), ip }
}
