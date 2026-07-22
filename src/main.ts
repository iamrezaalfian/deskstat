import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";

// ---------- types ----------
interface SystemStats {
  cpu_total: number;
  cpu_per_core: number[];
  mem_used_gb: number;
  mem_total_gb: number;
  mem_pct: number;
  cpu_temp: number | null;
  gpu_temp: number | null;
  ssd_temp: number | null;
  battery_pct: number | null;
  battery_status: string | null;
  top_cpu: [string, number][];
  top_mem: [string, number][];
  disk_used_gb: number;
  disk_total_gb: number;
  net_down_kbps: number;
  net_up_kbps: number;
  net_iface: string | null;
  load_avg_1: number;
  load_avg_5: number;
  load_avg_15: number;
  uptime_secs: number;
  process_count: number;
}

interface ClaudeQuota {
  five_hour_pct: number | null;
  five_hour_resets_at: string | null;
  weekly_pct: number | null;
  weekly_resets_at: string | null;
}

interface VpnStatus {
  connected: boolean;
  name: string | null;
  ip: string | null;
}

interface PlaneIssue {
  id: string;
  name: string;
  state_id: string | null;
  state_name: string | null;
  state_color: string | null;
  project_label: string;
  updated_at: string | null;
}

interface PlaneState {
  id: string;
  name: string;
  color: string;
  group: string;
}

interface PlaneIssuesResult {
  issues: PlaneIssue[];
  states_by_project: Record<string, PlaneState[]>;
}

interface PlaneProjectConfig {
  label: string;
  workspace_slug: string;
  project_id: string;
}

interface PlaneSettings {
  base_url: string;
  api_token: string;
  projects: PlaneProjectConfig[];
}

interface PowerTimerStatus {
  active: boolean;
  action: string | null;
  remaining_secs: number;
}

// ---------- window sizing ----------
// VPN card adds a details block when connected — window is just tall enough
// for each case, picked by that one condition.
const HEIGHT_VPN_OFF = 600;
const HEIGHT_VPN_ON = 670;
let lastAppliedHeight = 0;

async function setWindowHeightForVpn(connected: boolean) {
  const target = connected ? HEIGHT_VPN_ON : HEIGHT_VPN_OFF;
  if (target === lastAppliedHeight) return;
  lastAppliedHeight = target;
  await getCurrentWindow().setSize(new LogicalSize(340, target));
}

// ---------- tabs ----------
function initTabs() {
  const tabs = document.querySelectorAll<HTMLButtonElement>(".tab");
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      tabs.forEach((t) => t.classList.remove("active"));
      document.querySelectorAll(".tab-panel").forEach((p) => p.classList.remove("active"));
      tab.classList.add("active");
      document.querySelector(`#tab-${tab.dataset.tab}`)?.classList.add("active");
    });
  });
}

// ---------- stats ----------
// status band: green under warn, yellow [warn,bad), red at/above bad
function statusClass(value: number, warn: number, bad: number): "" | "warn" | "bad" {
  if (value >= bad) return "bad";
  if (value >= warn) return "warn";
  return "";
}

function setBar(barId: string, pct: number, warn: number, bad: number) {
  const el = document.getElementById(barId);
  if (!el) return;
  el.style.width = `${Math.min(100, Math.max(0, pct)).toFixed(0)}%`;
  el.className = `bar-fill ${statusClass(pct, warn, bad)}`.trim();
}

function fmtKbps(kbps: number): string {
  if (kbps >= 1024) return `${(kbps / 1024).toFixed(1)} MB/s`;
  return `${kbps.toFixed(0)} KB/s`;
}

function fmtUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function fmtCountdown(iso: string | null): string {
  if (!iso) return "";
  const diffMs = new Date(iso).getTime() - Date.now();
  if (diffMs <= 0) return "resetting…";
  const totalMin = Math.floor(diffMs / 60000);
  const d = Math.floor(totalMin / 1440);
  const h = Math.floor((totalMin % 1440) / 60);
  const m = totalMin % 60;
  if (d > 0) return `resets in ${d}d ${h}h`;
  if (h > 0) return `resets in ${h}h ${m}m`;
  return `resets in ${m}m`;
}

// bar/number show REMAINING%, not used% — a fuller bar means more quota
// left, so the danger threshold flips: red when remaining is LOW.
function setRemainingBar(barId: string, remainingPct: number) {
  const el = document.getElementById(barId);
  if (!el) return;
  el.style.width = `${Math.min(100, Math.max(0, remainingPct)).toFixed(0)}%`;
  let cls = "";
  if (remainingPct <= 10) cls = "bad";
  else if (remainingPct <= 30) cls = "warn";
  el.className = `bar-fill ${cls}`.trim();
}

async function refreshClaudeQuota() {
  const q = await invoke<ClaudeQuota>("get_claude_quota");
  const set = (id: string, val: string) => {
    const el = document.getElementById(id);
    if (el) el.textContent = val;
  };

  if (q.five_hour_pct != null) {
    const remaining = 100 - q.five_hour_pct;
    set("q5-pct", `${remaining}%`);
    setRemainingBar("q5-bar", remaining);
  } else {
    set("q5-pct", "—");
  }
  set("q5-reset", fmtCountdown(q.five_hour_resets_at));

  if (q.weekly_pct != null) {
    const remaining = 100 - q.weekly_pct;
    set("qw-pct", `${remaining}%`);
    setRemainingBar("qw-bar", remaining);
  } else {
    set("qw-pct", "—");
  }
  set("qw-reset", fmtCountdown(q.weekly_resets_at));
}

async function refreshSystemStats() {
  const s = await invoke<SystemStats>("get_system_stats");
  const set = (id: string, val: string) => {
    const el = document.getElementById(id);
    if (el) el.textContent = val;
  };

  set("sys-cpu-pct", `${s.cpu_total.toFixed(0)}%`);
  setBar("sys-cpu-bar", s.cpu_total, 70, 90);

  set("sys-mem-pct", `${s.mem_used_gb.toFixed(1)}G / ${s.mem_total_gb.toFixed(1)}G`);
  setBar("sys-mem-bar", s.mem_pct, 70, 90);

  const diskPct = s.disk_total_gb > 0 ? (s.disk_used_gb / s.disk_total_gb) * 100 : 0;
  set("sys-disk-pct", `${s.disk_used_gb.toFixed(0)}G / ${s.disk_total_gb.toFixed(0)}G`);
  setBar("sys-disk-bar", diskPct, 80, 92);

  const setTemp = (id: string, t: number | null) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.textContent = t != null ? `${t.toFixed(0)}°C` : "—";
    el.className = `stat-value ${t != null ? statusClass(t, 70, 85) : ""}`.trim();
  };
  setTemp("sys-cputemp", s.cpu_temp);
  setTemp("sys-gputemp", s.gpu_temp);
  setTemp("sys-ssdtemp", s.ssd_temp);

  const battEl = document.getElementById("sys-battery");
  if (battEl) {
    battEl.textContent = s.battery_pct != null ? `${s.battery_pct}% ${s.battery_status ?? ""}` : "—";
    const discharging = (s.battery_status ?? "").toLowerCase() === "discharging";
    const cls = s.battery_pct != null && discharging ? statusClass(100 - s.battery_pct, 70, 85) : "";
    battEl.className = `stat-value ${cls}`.trim();
  }

  set("sys-net", `↓${fmtKbps(s.net_down_kbps)} ↑${fmtKbps(s.net_up_kbps)}`);
  set("sys-uptime", fmtUptime(s.uptime_secs));
}

// ---------- vpn ----------
async function refreshVpnStatus() {
  const v = await invoke<VpnStatus>("get_vpn_status");
  const card = document.getElementById("vpn-card");
  const dot = document.getElementById("vpn-dot");
  const statusText = document.getElementById("vpn-status");
  const details = document.getElementById("vpn-details");
  if (!card || !dot || !statusText || !details) return;

  card.classList.toggle("connected", v.connected);
  dot.classList.toggle("connected", v.connected);
  statusText.textContent = v.connected ? "Connected" : "Not connected";
  details.classList.toggle("show", v.connected);

  if (v.connected) {
    const nameEl = document.getElementById("vpn-name");
    const ipEl = document.getElementById("vpn-ip");
    if (nameEl) nameEl.textContent = v.name ?? "—";
    if (ipEl) ipEl.textContent = v.ip ?? "—";
  }
  setWindowHeightForVpn(v.connected);
}

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

// ---------- plane ----------
let planeProjects: PlaneProjectConfig[] = [];
let lastPlaneResult: PlaneIssuesResult = { issues: [], states_by_project: {} };
let planeOnlyCompleted = false;
let planeFilterProject = "";
let planeSearchQuery = "";

function fmtRelativeTime(iso: string | null): string {
  if (!iso) return "";
  const diffMs = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

async function populatePlaneFilter() {
  const settings = await invoke<PlaneSettings>("get_plane_settings");
  planeProjects = settings.projects;
  const picker = document.getElementById("plane-project-filter") as HTMLSelectElement | null;
  if (!picker) return;
  const options = planeProjects.map((p) => `<option value="${escapeHtml(p.label)}">${escapeHtml(p.label)}</option>`).join("");
  picker.innerHTML = `<option value="">All workspaces</option>${options}`;
}

function planeStateSelect(issue: PlaneIssue, states: PlaneState[]): string {
  const color = issue.state_color ?? "#898781";
  if (states.length === 0) {
    return issue.state_name
      ? `<span class="plane-status-wrap"><span class="plane-status-dot" style="background:${color}"></span><span class="plane-status-static">${escapeHtml(issue.state_name)}</span></span>`
      : "";
  }
  const options = states
    .map((s) => `<option value="${s.id}" ${s.id === issue.state_id ? "selected" : ""}>${escapeHtml(s.name)}</option>`)
    .join("");
  return `
    <span class="plane-status-wrap">
      <span class="plane-status-dot" style="background:${color}"></span>
      <select class="plane-status-select" data-project="${escapeHtml(issue.project_label)}" data-issue="${issue.id}">
        ${options}
      </select>
    </span>`;
}

function renderPlaneList() {
  const list = document.getElementById("plane-list")!;
  const q = planeSearchQuery.trim().toLowerCase();
  const filtered = lastPlaneResult.issues.filter((i) => {
    if (planeFilterProject && i.project_label !== planeFilterProject) return false;
    if (q && !i.name.toLowerCase().includes(q)) return false;
    return true;
  });

  if (filtered.length === 0) {
    list.innerHTML = `<li class="plane-empty">No issues.</li>`;
    return;
  }

  list.innerHTML = filtered
    .map((i) => {
      const state = planeStateSelect(i, lastPlaneResult.states_by_project[i.project_label] ?? []);
      return `
        <li class="plane-card">
          <div class="plane-card-top">${state}</div>
          <div class="plane-card-title">${escapeHtml(i.name)}</div>
          <div class="plane-card-footer">
            <span class="plane-card-workspace">${escapeHtml(i.project_label)}</span>
            <span class="plane-card-updated">${fmtRelativeTime(i.updated_at)}</span>
          </div>
        </li>`;
    })
    .join("");
}

async function refreshPlaneIssues() {
  const errEl = document.getElementById("plane-error")!;
  errEl.textContent = "";
  try {
    lastPlaneResult = await invoke<PlaneIssuesResult>("plane_fetch_issues", { onlyCompleted: planeOnlyCompleted });
    renderPlaneList();
  } catch (e) {
    errEl.textContent = String(e);
    lastPlaneResult = { issues: [], states_by_project: {} };
    renderPlaneList();
  }
}

async function initPlaneCreate() {
  await populatePlaneFilter();

  document.getElementById("plane-project-filter")?.addEventListener("change", (e) => {
    planeFilterProject = (e.target as HTMLSelectElement).value;
    renderPlaneList();
  });

  document.getElementById("plane-search")?.addEventListener("input", (e) => {
    planeSearchQuery = (e.target as HTMLInputElement).value;
    renderPlaneList();
  });

  document.getElementById("plane-toggle-done")?.addEventListener("click", (e) => {
    planeOnlyCompleted = !planeOnlyCompleted;
    (e.currentTarget as HTMLButtonElement).classList.toggle("active", planeOnlyCompleted);
    refreshPlaneIssues();
  });

  const createForm = document.getElementById("plane-create-form");
  document.getElementById("plane-toggle-create")?.addEventListener("click", () => {
    createForm?.classList.toggle("plane-create-form-hidden");
    if (!createForm?.classList.contains("plane-create-form-hidden")) {
      document.getElementById("plane-new-name")?.focus();
    }
  });

  createForm?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const nameEl = document.getElementById("plane-new-name") as HTMLInputElement;
    const name = nameEl.value.trim();
    const targetProject = planeFilterProject || planeProjects[0]?.label;
    if (!name || !targetProject) return;

    const errEl = document.getElementById("plane-error")!;
    errEl.textContent = "";
    try {
      await invoke("plane_create_issue", { name, projectLabel: targetProject });
      nameEl.value = "";
      await refreshPlaneIssues();
    } catch (err) {
      errEl.textContent = String(err);
    }
  });
}

// ---------- power timer ----------
let selectedPowerAction: string | null = null;
let powerCountdownInterval: number | null = null;

function fmtCountdownSecs(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function showPowerActive(action: string, remainingSecs: number) {
  const active = document.getElementById("power-active");
  const actionEl = document.getElementById("power-active-action");
  const countdownEl = document.getElementById("power-countdown");
  if (active) active.style.display = "flex";
  if (actionEl) actionEl.textContent = action;
  if (countdownEl) countdownEl.textContent = fmtCountdownSecs(remainingSecs);
}

function hidePowerActive() {
  const active = document.getElementById("power-active");
  if (active) active.style.display = "none";
  if (powerCountdownInterval != null) {
    clearInterval(powerCountdownInterval);
    powerCountdownInterval = null;
  }
}

async function refreshPowerStatus() {
  const status = await invoke<PowerTimerStatus>("get_power_timer_status");
  if (status.active && status.action) {
    showPowerActive(status.action, status.remaining_secs);
    if (powerCountdownInterval == null) {
      let remaining = status.remaining_secs;
      powerCountdownInterval = window.setInterval(() => {
        remaining -= 1;
        if (remaining <= 0) {
          hidePowerActive();
          return;
        }
        const countdownEl = document.getElementById("power-countdown");
        if (countdownEl) countdownEl.textContent = fmtCountdownSecs(remaining);
      }, 1000);
    }
  } else {
    hidePowerActive();
  }
}

function initPower() {
  const buttons = document.querySelectorAll<HTMLButtonElement>(".power-action-btn");
  buttons.forEach((btn) => {
    btn.addEventListener("click", () => {
      buttons.forEach((b) => b.classList.remove("selected"));
      btn.classList.add("selected");
      selectedPowerAction = btn.dataset.action ?? null;
    });
  });

  document.getElementById("power-start")?.addEventListener("click", async () => {
    if (!selectedPowerAction) return;
    const minutesEl = document.getElementById("power-minutes") as HTMLInputElement;
    const minutes = parseInt(minutesEl.value, 10);
    if (!minutes || minutes < 1) return;
    await invoke("start_power_timer", { action: selectedPowerAction, minutes });
    refreshPowerStatus();
  });

  document.getElementById("power-cancel")?.addEventListener("click", async () => {
    await invoke("cancel_power_timer");
    hidePowerActive();
  });

  refreshPowerStatus();
}

// ---------- boot ----------
window.addEventListener("DOMContentLoaded", () => {
  initTabs();
  initPower();
  initPlaneCreate();

  refreshClaudeQuota();
  refreshSystemStats();
  refreshVpnStatus();
  setInterval(refreshSystemStats, 2000);
  setInterval(refreshClaudeQuota, 15000);
  setInterval(refreshVpnStatus, 5000);

  document.getElementById("plane-refresh")?.addEventListener("click", refreshPlaneIssues);

  document.getElementById("plane-list")?.addEventListener("change", async (e) => {
    const sel = e.target as HTMLSelectElement;
    if (!sel.classList.contains("plane-status-select")) return;
    const projectLabel = sel.dataset.project!;
    const issueId = sel.dataset.issue!;
    const stateId = sel.value;
    sel.disabled = true;
    try {
      await invoke("plane_update_issue_state", { projectLabel, issueId, stateId });
      await refreshPlaneIssues();
    } catch (err) {
      document.getElementById("plane-error")!.textContent = String(err);
      sel.disabled = false;
    }
  });

  document.getElementById("refresh-btn")?.addEventListener("click", async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.classList.add("spinning");
    await Promise.all([refreshClaudeQuota(), refreshSystemStats(), refreshVpnStatus()]);
    btn.classList.remove("spinning");
  });

  const win = getCurrentWindow();

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") win.hide();
  });

  // Click-outside-to-close, retried: a bare blur listener hides the window
  // before it's ever seen, because GTK fires a spurious focus-lost right as
  // the window is still realizing. The Rust side emits "window-shown" right
  // after show()+set_focus(); blur is ignored until a short grace period
  // past that timestamp has elapsed.
  let shownAt = 0;
  const BLUR_GRACE_MS = 400;
  listen("window-shown", () => {
    shownAt = Date.now();
  });

  // A native <select>'s open dropdown is its own top-level GTK window —
  // opening it steals window focus the same way a real click-away would,
  // so it'd otherwise trigger the same blur-hide. Suppress it while any
  // select has DOM focus (focus/blur don't bubble, focusin/focusout do).
  let selectFocused = false;
  document.addEventListener("focusin", (e) => {
    if ((e.target as HTMLElement)?.tagName === "SELECT") selectFocused = true;
  });
  document.addEventListener("focusout", (e) => {
    if ((e.target as HTMLElement)?.tagName === "SELECT") selectFocused = false;
  });

  win.onFocusChanged(({ payload: focused }) => {
    if (!focused && !selectFocused && Date.now() - shownAt > BLUR_GRACE_MS) {
      win.hide();
    }
  });
});
