use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

/// Public API: restart policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Never,
    #[serde(rename = "on-failure")]
    OnFailure,
    Always,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        RestartPolicy::Never
    }
}

/// Public API: config mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigMode {
    None,
    Path,
    Inline,
}

impl Default for ConfigMode {
    fn default() -> Self {
        ConfigMode::None
    }
}

/// Public API: runtime status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstanceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Exited,
    Error,
}

impl Default for InstanceStatus {
    fn default() -> Self {
        InstanceStatus::Stopped
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstanceRuntime {
    pub status: InstanceStatus,
    pub pid: Option<u32>,
    pub started_at: Option<String>, // RFC3339
    pub exit_code: Option<i32>,
    pub cpu_percent: Option<f32>,
    pub mem_bytes: Option<u64>,
    pub clients_attached: u32,
}

impl Default for InstanceRuntime {
    fn default() -> Self {
        Self {
            status: InstanceStatus::Stopped,
            pid: None,
            started_at: None,
            exit_code: None,
            cpu_percent: None,
            mem_bytes: None,
            clients_attached: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: Uuid,
    pub created_at: String,
    pub updated_at: String,

    pub name: String,
    pub enabled: bool,

    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub use_pty: bool,

    pub config_mode: ConfigMode,
    pub config_path: Option<String>,
    pub config_filename: Option<String>,
    pub config_content: Option<String>,

    pub restart_policy: RestartPolicy,
    pub auto_start: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<InstanceRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceCreateRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,

    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub use_pty: bool,

    #[serde(default)]
    pub config_mode: ConfigMode,
    pub config_path: Option<String>,
    pub config_filename: Option<String>,
    pub config_content: Option<String>,

    #[serde(default)]
    pub restart_policy: RestartPolicy,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceUpdateRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,

    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub cwd: Option<Option<String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub use_pty: Option<bool>,

    pub config_mode: Option<ConfigMode>,
    pub config_path: Option<Option<String>>,
    pub config_filename: Option<Option<String>>,
    pub config_content: Option<Option<String>>,

    pub restart_policy: Option<RestartPolicy>,
    pub auto_start: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceEnvelope {
    pub instance: Instance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceListEnvelope {
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRuntimeEnvelope {
    pub id: Uuid,
    pub runtime: InstanceRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub bind_address: String,
    pub port: u16,
    pub data_dir: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsUpdateRequest {
    pub bind_address: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRotateResponse {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairStartRequest {
    pub base_url: Option<String>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairStartResponse {
    pub pair_id: String,
    pub pair_secret: String,
    pub pair_uri: String,
    pub expires_at_epoch: i64,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairCompleteRequest {
    pub pair_id: String,
    pub pair_secret: String,
    pub device_name: String,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairCompleteResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPairSession {
    pub pair_id: String,
    pub requested_name: Option<String>,
    pub platform: Option<String>,
    pub created_at: String,
    pub expires_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPairSessionsResponse {
    pub sessions: Vec<PendingPairSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairDecisionRequest {
    pub pair_id: String,
    pub decision: PairDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairDecisionResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDevice {
    pub id: String,
    pub name: String,
    pub platform: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDeviceListResponse {
    pub devices: Vec<AuthDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub mode: ConfigMode,
    pub path: Option<String>,
    pub filename: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfigEnvelope {
    pub id: Uuid,
    pub config: InstanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfigUpdateRequest {
    pub mode: ConfigMode,
    pub path: Option<String>,
    pub filename: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceOutputTailEnvelope {
    pub id: Uuid,
    pub bytes: usize,
    pub encoding: String, // "base64"
    pub data: String,
    pub truncated: bool,
}

fn default_true() -> bool {
    true
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn now_unix_seconds() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// DB row mapping (internal)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstanceRow {
    pub id: String,
    pub name: String,
    pub enabled: i64,

    pub command: String,
    pub args_json: String,
    pub cwd: Option<String>,
    pub env_json: String,
    pub use_pty: i64,

    pub config_mode: String,
    pub config_path: Option<String>,
    pub config_filename: Option<String>,
    pub config_content: Option<String>,

    pub restart_policy: String,
    pub auto_start: i64,

    pub created_at: String,
    pub updated_at: String,
}

impl InstanceRow {
    pub fn to_instance(self, runtime: Option<InstanceRuntime>) -> Result<Instance, AppError> {
        let id = Uuid::parse_str(&self.id).map_err(|_| AppError::internal("invalid uuid in db"))?;

        let args: Vec<String> = serde_json::from_str(&self.args_json)?;
        let env: BTreeMap<String, String> = serde_json::from_str(&self.env_json)?;

        let config_mode: ConfigMode =
            serde_json::from_str(&format!("\"{}\"", self.config_mode)).unwrap_or(ConfigMode::None);

        let restart_policy: RestartPolicy =
            serde_json::from_str(&format!("\"{}\"", self.restart_policy)).unwrap_or_default();

        Ok(Instance {
            id,
            created_at: self.created_at,
            updated_at: self.updated_at,

            name: self.name,
            enabled: self.enabled != 0,

            command: self.command,
            args,
            cwd: self.cwd,
            env,
            use_pty: self.use_pty != 0,

            config_mode,
            config_path: self.config_path,
            config_filename: self.config_filename,
            config_content: self.config_content,

            restart_policy,
            auto_start: self.auto_start != 0,

            runtime,
        })
    }
}

pub fn config_mode_to_db(mode: &ConfigMode) -> &'static str {
    match mode {
        ConfigMode::None => "none",
        ConfigMode::Path => "path",
        ConfigMode::Inline => "inline",
    }
}

pub fn restart_policy_to_db(p: &RestartPolicy) -> &'static str {
    match p {
        RestartPolicy::Never => "never",
        RestartPolicy::OnFailure => "on-failure",
        RestartPolicy::Always => "always",
    }
}

pub fn parse_config_mode_db(s: &str) -> ConfigMode {
    match s {
        "none" => ConfigMode::None,
        "path" => ConfigMode::Path,
        "inline" => ConfigMode::Inline,
        _ => ConfigMode::None,
    }
}

pub fn parse_restart_policy_db(s: &str) -> RestartPolicy {
    match s {
        "never" => RestartPolicy::Never,
        "on-failure" => RestartPolicy::OnFailure,
        "always" => RestartPolicy::Always,
        _ => RestartPolicy::Never,
    }
}
