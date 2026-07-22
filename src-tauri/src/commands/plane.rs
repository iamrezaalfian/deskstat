use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PlaneProject {
    pub label: String,
    pub workspace_slug: String,
    pub project_id: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PlaneSettings {
    pub base_url: String,
    pub api_token: String,
    pub projects: Vec<PlaneProject>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PlaneState {
    pub id: String,
    pub name: String,
    pub color: String,
    pub group: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PlaneIssue {
    pub id: String,
    pub name: String,
    pub state_id: Option<String>,
    pub state_name: Option<String>,
    pub state_color: Option<String>,
    pub project_label: String,
    pub updated_at: Option<String>,
}

#[derive(Serialize, Clone, Default)]
pub struct PlaneIssuesResult {
    pub issues: Vec<PlaneIssue>,
    // available states per project, keyed by project_label — lets the
    // frontend build a status dropdown without a separate round trip
    pub states_by_project: HashMap<String, Vec<PlaneState>>,
}

fn settings_path(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("plane_settings.json")
}

#[tauri::command]
pub fn get_plane_settings(app: AppHandle) -> PlaneSettings {
    let path = settings_path(&app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(PlaneSettings {
            base_url: "https://projects.digitalteam.id".into(),
            api_token: String::new(),
            projects: Vec::new(),
        })
}

#[tauri::command]
pub fn save_plane_settings(app: AppHandle, settings: PlaneSettings) -> Result<(), String> {
    let path = settings_path(&app);
    let s = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(path, s).map_err(|e| e.to_string())
}

fn issues_url(base_url: &str, project: &PlaneProject) -> String {
    format!(
        "{}/api/v1/workspaces/{}/projects/{}/issues/",
        base_url.trim_end_matches('/'),
        project.workspace_slug,
        project.project_id
    )
}

fn issue_url(base_url: &str, project: &PlaneProject, issue_id: &str) -> String {
    format!("{}{}", issues_url(base_url, project), issue_id)
}

fn states_url(base_url: &str, project: &PlaneProject) -> String {
    format!(
        "{}/api/v1/workspaces/{}/projects/{}/states/",
        base_url.trim_end_matches('/'),
        project.workspace_slug,
        project.project_id
    )
}

// Issues carry only a state id — the readable name, color, and group (which
// drives the "skip done" filter) live on the project's states list, fetched
// once per project per refresh.
async fn fetch_states(client: &reqwest::Client, base_url: &str, api_token: &str, project: &PlaneProject) -> Vec<PlaneState> {
    let Ok(resp) = client
        .get(states_url(base_url, project))
        .header("X-API-Key", api_token)
        .send()
        .await
    else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    let results = body.get("results").unwrap_or(&body);
    results
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some(PlaneState {
                        id: s.get("id")?.as_str()?.to_string(),
                        name: s.get("name")?.as_str()?.to_string(),
                        color: s.get("color").and_then(|v| v.as_str()).unwrap_or("#898781").to_string(),
                        group: s.get("group").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_current_user_id(client: &reqwest::Client, base_url: &str, api_token: &str) -> Option<String> {
    let resp = client
        .get(format!("{}/api/v1/users/me/", base_url.trim_end_matches('/')))
        .header("X-API-Key", api_token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("id").and_then(|v| v.as_str()).map(str::to_string)
}

// Filtering by assignee happens client-side against the fetched page rather
// than via a query param — Plane's issues endpoint doesn't take one uniformly
// across self-hosted versions, but every issue already carries its assignee
// id list, so this is a plain post-filter of the fetch's own response body.
fn issue_assigned_to(v: &serde_json::Value, my_id: &str) -> bool {
    v.get("assignees")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().any(|a| a.as_str() == Some(my_id)))
        .unwrap_or(false)
}

async fn fetch_project_issues(
    client: &reqwest::Client,
    base_url: &str,
    api_token: &str,
    project: &PlaneProject,
    my_id: Option<&str>,
    only_completed: bool,
) -> Result<(Vec<PlaneIssue>, Vec<PlaneState>), String> {
    let resp = client
        .get(issues_url(base_url, project))
        .header("X-API-Key", api_token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Plane returned {}", resp.status()));
    }

    let states = fetch_states(client, base_url, api_token, project).await;
    let state_by_id: HashMap<&str, &PlaneState> = states.iter().map(|s| (s.id.as_str(), s)).collect();

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let results = body.get("results").unwrap_or(&body);
    let issues = results
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|v| my_id.map(|id| issue_assigned_to(v, id)).unwrap_or(true))
                .filter(|v| {
                    let state_id = v.get("state").and_then(|s| s.as_str());
                    let group = state_id.and_then(|id| state_by_id.get(id)).map(|s| s.group.as_str());
                    let is_done = matches!(group, Some("completed") | Some("cancelled"));
                    if only_completed { is_done } else { !is_done }
                })
                .filter_map(|v| {
                    let state_id = v.get("state").and_then(|s| s.as_str());
                    let state = state_id.and_then(|id| state_by_id.get(id));
                    Some(PlaneIssue {
                        id: v.get("id")?.as_str()?.to_string(),
                        name: v.get("name")?.as_str()?.to_string(),
                        state_id: state_id.map(str::to_string),
                        state_name: state.map(|s| s.name.clone()),
                        state_color: state.map(|s| s.color.clone()),
                        project_label: project.label.clone(),
                        updated_at: v.get("updated_at").and_then(|s| s.as_str()).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok((issues, states))
}

// Fetches every configured project independently — one project down (bad
// token scope, deleted project, etc.) doesn't blank the whole list, it just
// drops that project's issues and surfaces the error for it. Default view is
// active work only; only_completed flips it to an exclusive completed/cancelled
// view rather than adding done issues on top of the active list.
#[tauri::command]
pub async fn plane_fetch_issues(app: AppHandle, only_completed: bool) -> Result<PlaneIssuesResult, String> {
    let settings = get_plane_settings(app);
    if settings.api_token.is_empty() || settings.projects.is_empty() {
        return Err("Plane not configured yet — add a project.".into());
    }

    let client = reqwest::Client::new();
    let my_id = fetch_current_user_id(&client, &settings.base_url, &settings.api_token).await;

    let mut result = PlaneIssuesResult::default();
    let mut errors = Vec::new();

    for project in &settings.projects {
        match fetch_project_issues(&client, &settings.base_url, &settings.api_token, project, my_id.as_deref(), only_completed).await {
            Ok((mut issues, states)) => {
                result.issues.append(&mut issues);
                result.states_by_project.insert(project.label.clone(), states);
            }
            Err(e) => errors.push(format!("{}: {e}", project.label)),
        }
    }

    if result.issues.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(result)
}

#[tauri::command]
pub async fn plane_create_issue(app: AppHandle, name: String, project_label: String) -> Result<PlaneIssue, String> {
    let settings = get_plane_settings(app);
    if settings.api_token.is_empty() {
        return Err("Plane not configured yet — add a project.".into());
    }
    let project = settings
        .projects
        .iter()
        .find(|p| p.label == project_label)
        .ok_or_else(|| format!("unknown project: {project_label}"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post(issues_url(&settings.base_url, project))
        .header("X-API-Key", &settings.api_token)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Plane returned {}", resp.status()));
    }

    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(PlaneIssue {
        id: v.get("id").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
        name: v.get("name").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
        state_id: None,
        state_name: None,
        state_color: None,
        project_label: project.label.clone(),
        updated_at: v.get("updated_at").and_then(|s| s.as_str()).map(str::to_string),
    })
}

#[tauri::command]
pub async fn plane_update_issue_state(
    app: AppHandle,
    project_label: String,
    issue_id: String,
    state_id: String,
) -> Result<(), String> {
    let settings = get_plane_settings(app);
    if settings.api_token.is_empty() {
        return Err("Plane not configured yet — add a project.".into());
    }
    let project = settings
        .projects
        .iter()
        .find(|p| p.label == project_label)
        .ok_or_else(|| format!("unknown project: {project_label}"))?;

    let client = reqwest::Client::new();
    let resp = client
        .patch(issue_url(&settings.base_url, project, &issue_id))
        .header("X-API-Key", &settings.api_token)
        .json(&serde_json::json!({ "state": state_id }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Plane returned {}", resp.status()));
    }

    Ok(())
}
