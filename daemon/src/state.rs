use crate::{config::DaemonConfig, process::ProcessManager};
use sqlx::SqlitePool;
use std::{
    sync::{Arc, RwLock},
    time::Instant,
};
use tokio::sync::watch;

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<DaemonConfig>>,
    pub db: SqlitePool,
    pub process: Arc<ProcessManager>,
    started_at: Instant,
    shutdown_tx: watch::Sender<bool>,
}

impl AppState {
    pub fn new(cfg: DaemonConfig, db: SqlitePool) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            config: Arc::new(RwLock::new(cfg)),
            db,
            process: Arc::new(ProcessManager::new()),
            started_at: Instant::now(),
            shutdown_tx,
        }
    }

    /// Snapshot the current config (cheap clone).
    pub fn config_read(&self) -> DaemonConfig {
        self.config.read().expect("config lock poisoned").clone()
    }

    pub fn config_write(&self) -> std::sync::RwLockWriteGuard<'_, DaemonConfig> {
        self.config.write().expect("config lock poisoned")
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn request_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }
}
