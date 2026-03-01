use crate::error::AppError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const DEFAULT_BIND: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8765;

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub bind_address: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub web_dir: PathBuf,
    pub token: String,

    config_file: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonConfigFile {
    bind_address: String,
    port: u16,
    token: String,
}

impl DaemonConfig {
    pub fn load_or_create() -> Result<Self, AppError> {
        let data_dir = resolve_data_dir()?;
        let web_dir = resolve_web_dir()?;
        std::fs::create_dir_all(&data_dir)?;

        let config_file = data_dir.join("daemon.json");

        let mut cfg = if config_file.exists() {
            let raw = std::fs::read_to_string(&config_file)?;
            let f: DaemonConfigFile = serde_json::from_str(&raw)?;
            DaemonConfig {
                bind_address: f.bind_address,
                port: f.port,
                token: f.token,
                data_dir,
                web_dir: web_dir.clone(),
                config_file,
            }
        } else {
            let token = Uuid::new_v4().to_string();
            let cfg = DaemonConfig {
                bind_address: DEFAULT_BIND.to_string(),
                port: DEFAULT_PORT,
                token,
                data_dir,
                web_dir: web_dir.clone(),
                config_file,
            };
            cfg.save()?; // create file
            cfg
        };

        // Env overrides (optional)
        if let Ok(v) = std::env::var("AICLI_BIND") {
            if !v.trim().is_empty() {
                cfg.bind_address = v;
            }
        }
        if let Ok(v) = std::env::var("AICLI_PORT") {
            if let Ok(p) = v.parse::<u16>() {
                cfg.port = p;
            }
        }
        if let Ok(v) = std::env::var("AICLI_TOKEN") {
            if !v.trim().is_empty() {
                cfg.token = v;
            }
        }

        Ok(cfg)
    }

    #[cfg(test)]
    pub fn for_tests(data_dir: PathBuf, web_dir: PathBuf, token: String) -> Self {
        Self {
            bind_address: DEFAULT_BIND.to_string(),
            port: DEFAULT_PORT,
            data_dir: data_dir.clone(),
            web_dir,
            token,
            config_file: data_dir.join("daemon.json"),
        }
    }

    pub fn save(&self) -> Result<(), AppError> {
        let f = DaemonConfigFile {
            bind_address: self.bind_address.clone(),
            port: self.port,
            token: self.token.clone(),
        };
        let raw = serde_json::to_string_pretty(&f)?;
        std::fs::write(&self.config_file, raw)?;
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("db.sqlite")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }
}

fn resolve_data_dir() -> Result<PathBuf, AppError> {
    if let Ok(v) = std::env::var("AICLI_DATA_DIR") {
        if !v.trim().is_empty() {
            return Ok(PathBuf::from(v));
        }
    }

    // Windows default: %APPDATA%/<org>/<app>
    let proj = ProjectDirs::from("com", "aicli", "ai-cli-manager")
        .ok_or_else(|| AppError::internal("cannot resolve data dir"))?;
    Ok(proj.data_dir().to_path_buf())
}

fn resolve_web_dir() -> Result<PathBuf, AppError> {
    if let Ok(v) = std::env::var("AICLI_WEB_DIR") {
        if !v.trim().is_empty() {
            return Ok(PathBuf::from(v));
        }
    }

    let exe = std::env::current_exe()
        .map_err(|e| AppError::internal(format!("cannot resolve current exe path: {e}")))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| AppError::internal("cannot resolve current exe dir"))?;
    Ok(exe_dir.join("web"))
}
