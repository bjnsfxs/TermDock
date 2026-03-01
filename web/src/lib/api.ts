import type {
  CreateOrUpdateInstanceRequest,
  DaemonSettings,
  GetInstanceResponse,
  ListInstancesResponse,
  RotateTokenResponse,
  UpdateSettingsRequest,
} from "./types";

export type ApiConfig = {
  baseUrl: string; // e.g. http://127.0.0.1:8765
  token: string; // bearer token
};

const FALLBACK_DAEMON_BASE_URL = "http://127.0.0.1:8765";

export class ApiError extends Error {
  status: number;
  code?: string;

  constructor(message: string, status: number, code?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

export function resolveDefaultBaseUrl(locationLike?: {
  origin?: string;
  protocol?: string;
}): string {
  const protocol = locationLike?.protocol?.toLowerCase();
  if ((protocol === "http:" || protocol === "https:") && locationLike?.origin) {
    return locationLike.origin;
  }
  return FALLBACK_DAEMON_BASE_URL;
}

export function loadApiConfig(): ApiConfig {
  const fromStorage = localStorage.getItem("daemonBaseUrl");
  const baseUrl = fromStorage || resolveDefaultBaseUrl(typeof window !== "undefined" ? window.location : undefined);
  const token = localStorage.getItem("daemonToken") || "";
  return { baseUrl, token };
}

export function saveApiConfig(cfg: ApiConfig) {
  localStorage.setItem("daemonBaseUrl", cfg.baseUrl);
  localStorage.setItem("daemonToken", cfg.token);
}

export function buildWsUrl(path: string): string {
  const { baseUrl, token } = loadApiConfig();
  const wsBase = baseUrl.replace(/^http/i, "ws").replace(/\/$/, "");
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  const withToken = token
    ? `${normalizedPath}${normalizedPath.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}`
    : normalizedPath;
  return `${wsBase}${withToken}`;
}

async function apiFetch(path: string, init?: RequestInit) {
  const { baseUrl, token } = loadApiConfig();
  const url = baseUrl.replace(/\/$/, "") + path;

  const headers = new Headers(init?.headers || {});
  if (token) headers.set("Authorization", `Bearer ${token}`);
  headers.set("Content-Type", "application/json");

  const res = await fetch(url, { ...init, headers });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    let code: string | undefined;
    try {
      const body = await res.json();
      if (body?.error?.message) msg = body.error.message;
      if (typeof body?.error?.code === "string") code = body.error.code;
    } catch {}
    throw new ApiError(msg, res.status, code);
  }
  if (res.status === 204) return null;
  return res.json();
}

export async function health() {
  const { baseUrl } = loadApiConfig();
  const res = await fetch(baseUrl.replace(/\/$/, "") + "/health");
  return res.json();
}

export async function listInstances(): Promise<ListInstancesResponse> {
  return apiFetch("/api/v1/instances?include_runtime=true");
}

export async function getInstance(id: string): Promise<GetInstanceResponse> {
  return apiFetch(`/api/v1/instances/${id}?include_runtime=true`);
}

export async function createInstance(payload: CreateOrUpdateInstanceRequest) {
  return apiFetch("/api/v1/instances", { method: "POST", body: JSON.stringify(payload) });
}

export async function updateInstance(id: string, payload: CreateOrUpdateInstanceRequest) {
  return apiFetch(`/api/v1/instances/${id}`, { method: "PUT", body: JSON.stringify(payload) });
}

export async function deleteInstance(id: string) {
  return apiFetch(`/api/v1/instances/${id}`, { method: "DELETE" });
}

export async function startInstance(id: string) {
  return apiFetch(`/api/v1/instances/${id}/start`, { method: "POST" });
}
export async function stopInstance(id: string) {
  return apiFetch(`/api/v1/instances/${id}/stop`, { method: "POST" });
}
export async function restartInstance(id: string) {
  return apiFetch(`/api/v1/instances/${id}/restart`, { method: "POST" });
}

export async function getSettings() {
  return apiFetch("/api/v1/settings") as Promise<DaemonSettings>;
}

export async function updateSettings(payload: UpdateSettingsRequest) {
  return apiFetch("/api/v1/settings", {
    method: "PUT",
    body: JSON.stringify(payload),
  }) as Promise<DaemonSettings>;
}

export async function rotateToken(): Promise<RotateTokenResponse> {
  return apiFetch("/api/v1/auth/token/rotate", { method: "POST" }) as Promise<RotateTokenResponse>;
}
