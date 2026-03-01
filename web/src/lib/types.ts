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
