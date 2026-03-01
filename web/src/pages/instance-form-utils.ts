import type { ConfigMode, CreateOrUpdateInstanceRequest, RestartPolicy } from "../lib/types";

export type InstanceFormState = {
  name: string;
  enabled: boolean;
  command: string;
  args: string[];
  cwd: string;
  use_pty: boolean;
  config_mode: ConfigMode;
  config_path: string;
  config_filename: string;
  config_content: string;
  restart_policy: RestartPolicy;
  auto_start: boolean;
};

export function parseArgsInput(raw: string): string[] {
  return raw.split(/\s+/).map((part) => part.trim()).filter(Boolean);
}

export function parseEnvJson(raw: string): Record<string, string> {
  const trimmed = raw.trim();
  if (!trimmed) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    throw new Error("Environment variables must be valid JSON.");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Environment variables must be a JSON object.");
  }

  const normalized: Record<string, string> = {};
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value !== "string") {
      throw new Error(`Environment variable "${key}" must be a string.`);
    }
    normalized[key] = value;
  }
  return normalized;
}

export function buildInstancePayload(form: InstanceFormState, envText: string): CreateOrUpdateInstanceRequest {
  const name = form.name.trim();
  const command = form.command.trim();
  if (!name) throw new Error("Instance name is required.");
  if (!command) throw new Error("Command is required.");

  const env = parseEnvJson(envText);
  const mode = form.config_mode;
  const configPath = mode === "path" ? form.config_path.trim() || null : null;
  const configFilename = mode === "inline" ? form.config_filename.trim() || "config.yaml" : null;
  const configContent = mode === "inline" ? form.config_content : null;

  return {
    name,
    enabled: form.enabled,
    command,
    args: form.args,
    cwd: form.cwd.trim() || null,
    env,
    use_pty: form.use_pty,
    config_mode: mode,
    config_path: configPath,
    config_filename: configFilename,
    config_content: configContent,
    restart_policy: form.restart_policy,
    auto_start: form.auto_start,
  };
}
