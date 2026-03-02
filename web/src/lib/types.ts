export type RuntimeStatus =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "exited"
  | "error"
  | (string & {});

export type ConfigMode = "none" | "path" | "inline" | (string & {});
export type RestartPolicy = "never" | "on-failure" | "always" | (string & {});

export type RuntimeSnapshot = {
  status: RuntimeStatus;
  pid: number | null;
  started_at: string | null;
  exit_code: number | null;
  cpu_percent: number | null;
  mem_bytes: number | null;
  clients_attached: number;
  backend?: string | null;
};

export type Instance = {
  id: string;
  name: string;
  enabled: boolean;
  command: string;
  args: string[];
  cwd: string | null;
  env: Record<string, string>;
  use_pty: boolean;
  config_mode: ConfigMode;
  config_path: string | null;
  config_filename: string | null;
  config_content: string | null;
  restart_policy: RestartPolicy;
  auto_start: boolean;
  created_at?: string;
  updated_at?: string;
  runtime?: Partial<RuntimeSnapshot> | null;
};

export type ListInstancesResponse = {
  instances: Instance[];
};

export type GetInstanceResponse = {
  instance: Instance;
};

export type DaemonSettings = {
  bind_address: string;
  port: number;
  data_dir: string;
  token: string;
};

export type RotateTokenResponse = {
  token: string;
};

export type ApiConfig = {
  baseUrl: string;
  token: string;
};

export type DaemonProfile = {
  id: string;
  label: string;
  baseUrl: string;
  token: string;
  deviceId?: string | null;
  lastSeenAt?: string | null;
};

export type UpdateSettingsRequest = {
  bind_address?: string;
  port?: number;
};

export type CreateOrUpdateInstanceRequest = {
  name: string;
  enabled: boolean;
  command: string;
  args: string[];
  cwd: string | null;
  env: Record<string, string>;
  use_pty: boolean;
  config_mode: ConfigMode;
  config_path: string | null;
  config_filename: string | null;
  config_content: string | null;
  restart_policy: RestartPolicy;
  auto_start: boolean;
};

export type PairStartRequest = {
  base_url?: string;
  ttl_seconds?: number;
};

export type PairStartResponse = {
  pair_id: string;
  pair_secret: string;
  pair_uri: string;
  expires_at_epoch: number;
  expires_in_seconds: number;
};

export type PairCompleteRequest = {
  pair_id: string;
  pair_secret: string;
  device_name: string;
  platform?: string;
};

export type PairCompleteResponse = {
  status: string;
};

export type PairStatusResponse = {
  status: string;
  device_id?: string;
  device_token?: string;
  message?: string;
};

export type PendingPairSession = {
  pair_id: string;
  requested_name?: string;
  platform?: string;
  created_at: string;
  expires_at_epoch: number;
};

export type PendingPairSessionsResponse = {
  sessions: PendingPairSession[];
};

export type PairDecision = "approve" | "reject";

export type PairDecisionRequest = {
  pair_id: string;
  decision: PairDecision;
};

export type PairDecisionResponse = {
  status: string;
  device_id?: string;
};

export type AuthDevice = {
  id: string;
  name: string;
  platform?: string;
  created_at: string;
  last_seen_at: string;
  revoked_at?: string | null;
};

export type AuthDeviceListResponse = {
  devices: AuthDevice[];
};

export type DaemonStatus = {
  reachable: boolean;
  managed: boolean;
  pid: number | null;
  baseUrl: string;
  message?: string | null;
};

export type DaemonActionResponse = {
  status: DaemonStatus;
};

export type EventsHelloMessage = {
  type: "hello";
  daemon_version?: string;
};

export type InstanceStatusMessage = {
  type: "instance_status";
  id: string;
  runtime: Partial<RuntimeSnapshot>;
};

export type EventsNoticeMessage = {
  type: "notice";
  message: string;
  level?: "info" | "warn" | "error" | string;
};

export type EventsPongMessage = {
  type: "pong";
};

export type EventsMessage =
  | EventsHelloMessage
  | InstanceStatusMessage
  | EventsNoticeMessage
  | EventsPongMessage
  | { type: string; [k: string]: unknown };

export type TerminalControlMessage =
  | {
      type: "hello";
      daemon_version?: string;
      client_id?: string;
      client_name?: string;
    }
  | {
      type: "status";
      id?: string;
      status?: RuntimeStatus;
      pid?: number | null;
      backend?: string;
      clients_attached?: number;
    }
  | {
      type: "error" | "warning";
      code?: string;
      message?: string;
    }
  | {
      type: "tail_begin";
      requested?: number;
      bytes?: number;
      truncated?: boolean;
    }
  | {
      type: "pong";
    }
  | {
      type: string;
      [k: string]: unknown;
    };
