use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use sysinfo::{Disks, Networks, System};

#[derive(Serialize, Clone)]
pub struct SystemStats {
    pub cpu_total: f32,
    pub cpu_per_core: Vec<f32>,
    pub mem_used_gb: f64,
    pub mem_total_gb: f64,
    pub mem_pct: f32,
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub ssd_temp: Option<f32>,
    pub battery_pct: Option<u8>,
    pub battery_status: Option<String>,
    pub top_cpu: Vec<(String, f32)>,
    pub top_mem: Vec<(String, u64)>,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub net_down_kbps: f64,
    pub net_up_kbps: f64,
    pub net_iface: Option<String>,
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
    pub uptime_secs: u64,
    pub process_count: usize,
}

// A fresh `System::new_all()` on every poll is expensive (full process/CPU
// rescan) and CPU deltas need two refreshes over real wall-clock time to be
// meaningful — so this state persists across calls instead of being rebuilt
// each time. First call after startup reports 0% CPU (no prior sample yet);
// it's accurate from the second poll on.
struct StatState {
    sys: System,
    disks: Disks,
    networks: Networks,
    last_net: HashMap<String, (u64, u64)>,
    last_sample: Instant,
}

fn state() -> &'static Mutex<StatState> {
    static STATE: OnceLock<Mutex<StatState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(StatState {
            sys: System::new_all(),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last_net: HashMap::new(),
            last_sample: Instant::now(),
        })
    })
}

fn read_sensors_temp(chip: &str, label: &str) -> Option<f32> {
    let out = Command::new("sensors").arg(chip).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if line.trim_start().starts_with(label) {
            let val = line.split_whitespace().nth(1)?;
            let cleaned: String = val.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            return cleaned.parse::<f32>().ok();
        }
    }
    None
}

fn read_battery() -> (Option<u8>, Option<String>) {
    let cap = fs::read_to_string("/sys/class/power_supply/BAT0/capacity")
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok());
    let status = fs::read_to_string("/sys/class/power_supply/BAT0/status")
        .ok()
        .map(|s| s.trim().to_string());
    (cap, status)
}

#[tauri::command]
pub fn get_system_stats() -> SystemStats {
    let mut st = state().lock().unwrap();

    st.sys.refresh_cpu_all();
    st.sys.refresh_memory();
    st.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    st.disks.refresh(true);
    st.networks.refresh(true);

    let cpu_per_core: Vec<f32> = st.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_total = if cpu_per_core.is_empty() {
        0.0
    } else {
        cpu_per_core.iter().sum::<f32>() / cpu_per_core.len() as f32
    };

    let mem_total_gb = st.sys.total_memory() as f64 / 1_073_741_824.0;
    let mem_used_gb = st.sys.used_memory() as f64 / 1_073_741_824.0;
    let mem_pct = if st.sys.total_memory() > 0 {
        (st.sys.used_memory() as f32 / st.sys.total_memory() as f32) * 100.0
    } else {
        0.0
    };

    let mut procs: Vec<_> = st.sys.processes().values().collect();
    procs.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap());
    let top_cpu: Vec<(String, f32)> = procs
        .iter()
        .take(3)
        .map(|p| (p.name().to_string_lossy().to_string(), p.cpu_usage()))
        .collect();

    procs.sort_by(|a, b| b.memory().cmp(&a.memory()));
    let top_mem: Vec<(String, u64)> = procs
        .iter()
        .take(3)
        .map(|p| (p.name().to_string_lossy().to_string(), p.memory() / 1024 / 1024))
        .collect();

    let process_count = st.sys.processes().len();

    let cpu_temp = read_sensors_temp("k10temp-pci-00c3", "Tctl");
    let gpu_temp = read_sensors_temp("amdgpu-pci-0700", "edge");
    let ssd_temp = read_sensors_temp("nvme-pci-0100", "Composite");
    let (battery_pct, battery_status) = read_battery();

    let (disk_used_gb, disk_total_gb) = st
        .disks
        .iter()
        .find(|d| d.mount_point().to_str() == Some("/"))
        .map(|d| {
            let total = d.total_space() as f64 / 1_073_741_824.0;
            let avail = d.available_space() as f64 / 1_073_741_824.0;
            (total - avail, total)
        })
        .unwrap_or((0.0, 0.0));

    // network throughput: biggest non-loopback interface currently carrying traffic
    let now = Instant::now();
    let elapsed = now.duration_since(st.last_sample).as_secs_f64().max(0.001);
    let mut net_down_kbps = 0.0;
    let mut net_up_kbps = 0.0;
    let mut net_iface = None;
    let mut best_delta = 0u64;
    let mut new_last_net = HashMap::new();

    for (name, data) in st.networks.iter() {
        if name == "lo" || name.starts_with("veth") || name.starts_with("br-") || name.starts_with("docker") {
            continue;
        }
        let rx = data.total_received();
        let tx = data.total_transmitted();
        new_last_net.insert(name.clone(), (rx, tx));

        if let Some(&(prev_rx, prev_tx)) = st.last_net.get(name) {
            let drx = rx.saturating_sub(prev_rx);
            let dtx = tx.saturating_sub(prev_tx);
            if drx + dtx >= best_delta {
                best_delta = drx + dtx;
                net_down_kbps = (drx as f64 / elapsed) / 1024.0;
                net_up_kbps = (dtx as f64 / elapsed) / 1024.0;
                net_iface = Some(name.clone());
            }
        }
    }
    st.last_net = new_last_net;
    st.last_sample = now;

    let load = System::load_average();
    let uptime_secs = System::uptime();

    SystemStats {
        cpu_total,
        cpu_per_core,
        mem_used_gb,
        mem_total_gb,
        mem_pct,
        cpu_temp,
        gpu_temp,
        ssd_temp,
        battery_pct,
        battery_status,
        top_cpu,
        top_mem,
        disk_used_gb,
        disk_total_gb,
        net_down_kbps,
        net_up_kbps,
        net_iface,
        load_avg_1: load.one,
        load_avg_5: load.five,
        load_avg_15: load.fifteen,
        uptime_secs,
        process_count,
    }
}
