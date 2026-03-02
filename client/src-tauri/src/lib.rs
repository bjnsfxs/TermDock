use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};

const FALLBACK_BASE_URL: &str = "http://127.0.0.1:8765";
const START_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Deserialize)]
struct DaemonConfigFile {
    bind_address: String,
    port: u16,
    token: String,
}

#[derive(Debug)]
struct ManagedDaemon {
    child: Child,
}

#[derive(Default)]
struct DaemonSupervisor {
    managed: Mutex<Option<ManagedDaemon>>,
}

impl Drop for DaemonSupervisor {
    fn drop(&mut self) {
        if let Ok(slot) = self.managed.get_mut() {
            if let Some(managed) = slot.as_mut() {
                let _ = managed.child.kill();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonStatus {
    reachable: bool,
    managed: bool,
    pid: Option<u32>,
    base_url: String,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonActionResponse {
    status: DaemonStatus,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DaemonSupervisor::default())
        .invoke_handler(tauri::generate_handler![
            daemon_status,
            daemon_start,
            daemon_stop,
            daemon_restart,
            daemon_bootstrap
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn daemon_status(
    supervisor: tauri::State<'_, DaemonSupervisor>,
) -> Result<DaemonStatus, String> {
    daemon_status_impl(&supervisor).await
}

#[tauri::command]
async fn daemon_start(
    supervisor: tauri::State<'_, DaemonSupervisor>,
) -> Result<DaemonActionResponse, String> {
    daemon_start_impl(&supervisor).await
}

#[tauri::command]
async fn daemon_stop(
    supervisor: tauri::State<'_, DaemonSupervisor>,
) -> Result<DaemonActionResponse, String> {
    daemon_stop_impl(&supervisor).await
}

#[tauri::command]
async fn daemon_restart(
    supervisor: tauri::State<'_, DaemonSupervisor>,
) -> Result<DaemonActionResponse, String> {
    daemon_stop_impl(&supervisor).await?;
    daemon_start_impl(&supervisor).await
}

#[tauri::command]
async fn daemon_bootstrap(
    supervisor: tauri::State<'_, DaemonSupervisor>,
) -> Result<DaemonActionResponse, String> {
    daemon_bootstrap_impl(&supervisor).await
}

async fn daemon_status_impl(supervisor: &DaemonSupervisor) -> Result<DaemonStatus, String> {
    cleanup_dead_child(supervisor)?;
    let endpoint = daemon_endpoint();
    let reachable = health_ok(&endpoint.base_url).await;
    let (managed, pid) = managed_process_info(supervisor)?;
    Ok(DaemonStatus {
        reachable,
        managed,
        pid,
        base_url: endpoint.base_url,
        message: None,
    })
}

async fn daemon_bootstrap_impl(
    supervisor: &DaemonSupervisor,
) -> Result<DaemonActionResponse, String> {
    let status = daemon_status_impl(supervisor).await?;
    if status.reachable {
        return Ok(DaemonActionResponse { status });
    }
    daemon_start_impl(supervisor).await
}

async fn daemon_start_impl(supervisor: &DaemonSupervisor) -> Result<DaemonActionResponse, String> {
    cleanup_dead_child(supervisor)?;
    let endpoint = daemon_endpoint();
    if health_ok(&endpoint.base_url).await {
        return Ok(DaemonActionResponse {
            status: DaemonStatus {
                reachable: true,
                managed: managed_process_info(supervisor)?.0,
                pid: managed_process_info(supervisor)?.1,
                base_url: endpoint.base_url,
                message: Some("daemon already running".to_string()),
            },
        });
    }

    let exe_path = find_daemon_executable().ok_or_else(|| {
        "cannot locate ai-cli-manager-daemon executable; set AICLI_DAEMON_EXE or build daemon first"
            .to_string()
    })?;

    let mut cmd = Command::new(&exe_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Ok(data_dir) = std::env::var("AICLI_DATA_DIR") {
        if !data_dir.trim().is_empty() {
            cmd.env("AICLI_DATA_DIR", data_dir);
        }
    }

    let child = cmd
        .spawn()
        .map_err(|err| format!("failed to start daemon: {err}"))?;

    {
        let mut guard = supervisor
            .managed
            .lock()
            .map_err(|_| "daemon state lock poisoned".to_string())?;
        *guard = Some(ManagedDaemon { child });
    }

    wait_for_health(&endpoint.base_url, START_TIMEOUT).await?;
    let (managed, pid) = managed_process_info(supervisor)?;
    Ok(DaemonActionResponse {
        status: DaemonStatus {
            reachable: true,
            managed,
            pid,
            base_url: endpoint.base_url,
            message: Some("daemon started".to_string()),
        },
    })
}

async fn daemon_stop_impl(supervisor: &DaemonSupervisor) -> Result<DaemonActionResponse, String> {
    cleanup_dead_child(supervisor)?;
    let endpoint = daemon_endpoint();

    if health_ok(&endpoint.base_url).await {
        let _ = request_shutdown(&endpoint).await;
        let _ = wait_for_not_healthy(&endpoint.base_url, STOP_TIMEOUT).await;
    }

    {
        let mut guard = supervisor
            .managed
            .lock()
            .map_err(|_| "daemon state lock poisoned".to_string())?;
        if let Some(mut managed) = guard.take() {
            let _ = managed.child.kill();
            let _ = managed.child.wait();
        }
    }

    let reachable = health_ok(&endpoint.base_url).await;
    if reachable {
        return Err("daemon is still reachable after stop attempt".to_string());
    }

    Ok(DaemonActionResponse {
        status: DaemonStatus {
            reachable: false,
            managed: false,
            pid: None,
            base_url: endpoint.base_url,
            message: Some("daemon stopped".to_string()),
        },
    })
}

fn cleanup_dead_child(supervisor: &DaemonSupervisor) -> Result<(), String> {
    let mut guard = supervisor
        .managed
        .lock()
        .map_err(|_| "daemon state lock poisoned".to_string())?;
    if let Some(managed) = guard.as_mut() {
        if managed
            .child
            .try_wait()
            .map_err(|err| format!("failed to poll managed daemon process: {err}"))?
            .is_some()
        {
            *guard = None;
        }
    }
    Ok(())
}

fn managed_process_info(supervisor: &DaemonSupervisor) -> Result<(bool, Option<u32>), String> {
    let guard = supervisor
        .managed
        .lock()
        .map_err(|_| "daemon state lock poisoned".to_string())?;
    if let Some(managed) = guard.as_ref() {
        Ok((true, Some(managed.child.id())))
    } else {
        Ok((false, None))
    }
}

async fn wait_for_health(base_url: &str, timeout: Duration) -> Result<(), String> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if health_ok(base_url).await {
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err(format!(
        "daemon did not become healthy within {} seconds",
        timeout.as_secs()
    ))
}

async fn wait_for_not_healthy(base_url: &str, timeout: Duration) -> Result<(), String> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if !health_ok(base_url).await {
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err(format!(
        "daemon remained healthy after {} seconds",
        timeout.as_secs()
    ))
}

async fn health_ok(base_url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok();
    let Some(client) = client else {
        return false;
    };
    match client.get(format!("{base_url}/health")).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn request_shutdown(endpoint: &DaemonEndpoint) -> Result<(), String> {
    let token = endpoint
        .token
        .as_ref()
        .ok_or_else(|| "missing daemon token".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|err| format!("failed to build http client: {err}"))?;

    let resp = client
        .post(format!("{}/api/v1/system/shutdown", endpoint.base_url))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|err| format!("failed to request daemon shutdown: {err}"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("shutdown endpoint returned {}", resp.status()))
    }
}

#[derive(Debug)]
struct DaemonEndpoint {
    base_url: String,
    token: Option<String>,
}

fn daemon_endpoint() -> DaemonEndpoint {
    if let Some(cfg) = load_daemon_config_file() {
        let host = if cfg.bind_address == "0.0.0.0" || cfg.bind_address == "::" {
            "127.0.0.1".to_string()
        } else {
            cfg.bind_address
        };
        return DaemonEndpoint {
            base_url: format!("http://{host}:{}", cfg.port),
            token: Some(cfg.token),
        };
    }

    DaemonEndpoint {
        base_url: FALLBACK_BASE_URL.to_string(),
        token: None,
    }
}

fn load_daemon_config_file() -> Option<DaemonConfigFile> {
    let data_dir = resolve_data_dir()?;
    let config_path = data_dir.join("daemon.json");
    let raw = std::fs::read_to_string(config_path).ok()?;
    serde_json::from_str::<DaemonConfigFile>(&raw).ok()
}

fn resolve_data_dir() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("AICLI_DATA_DIR") {
        if !v.trim().is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    let proj = ProjectDirs::from("com", "aicli", "ai-cli-manager")?;
    Some(proj.data_dir().to_path_buf())
}

fn find_daemon_executable() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("AICLI_DAEMON_EXE") {
        let path = PathBuf::from(v);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join("ai-cli-manager-daemon.exe"));
            candidates.push(exe_dir.join("bin").join("ai-cli-manager-daemon.exe"));
            if let Some(root) = find_workspace_root(exe_dir) {
                candidates.push(
                    root.join("daemon")
                        .join("target")
                        .join("debug")
                        .join("ai-cli-manager-daemon.exe"),
                );
                candidates.push(
                    root.join("daemon")
                        .join("target")
                        .join("release")
                        .join("ai-cli-manager-daemon.exe"),
                );
                candidates.push(
                    root.join("artifacts")
                        .join("ai-cli-manager-win-x64")
                        .join("bin")
                        .join("ai-cli-manager-daemon.exe"),
                );
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("daemon")
                .join("target")
                .join("debug")
                .join("ai-cli-manager-daemon.exe"),
        );
        candidates.push(
            cwd.join("daemon")
                .join("target")
                .join("release")
                .join("ai-cli-manager-daemon.exe"),
        );
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let daemon_manifest = ancestor.join("daemon").join("Cargo.toml");
        let root_package = ancestor.join("package.json");
        if daemon_manifest.is_file() && root_package.is_file() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}
