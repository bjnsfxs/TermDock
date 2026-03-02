import type {
  ApiConfig,
  AuthDeviceListResponse,
  CreateOrUpdateInstanceRequest,
  DaemonActionResponse,
  DaemonProfile,
  DaemonSettings,
  DaemonStatus,
  GetInstanceResponse,
  ListInstancesResponse,
  PairCompleteRequest,
  PairCompleteResponse,
  PairDecisionRequest,
  PairDecisionResponse,
  PairStartRequest,
  PairStartResponse,
  PairStatusResponse,
  PendingPairSessionsResponse,
  RotateTokenResponse,
  UpdateSettingsRequest,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: (command: string, args?: unknown) => Promise<unknown>;
    };
  }
}

const FALLBACK_DAEMON_BASE_URL = "http://127.0.0.1:8765";
const LEGACY_BASE_URL_KEY = "daemonBaseUrl";
const LEGACY_TOKEN_KEY = "daemonToken";
const PROFILES_KEY = "daemonProfilesV2";
const ACTIVE_PROFILE_ID_KEY = "activeDaemonProfileIdV2";

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
    try {
      const hostname = new URL(locationLike.origin).hostname.toLowerCase();

      // Tauri custom protocols commonly map to http(s)://<scheme>.localhost.
      if (hostname.endsWith(".localhost") && hostname !== "localhost") {
        return FALLBACK_DAEMON_BASE_URL;
      }

      return locationLike.origin;
    } catch {
      return FALLBACK_DAEMON_BASE_URL;
    }
  }
  return FALLBACK_DAEMON_BASE_URL;
}

export function listProfiles(): DaemonProfile[] {
  return ensureProfilesInitialized().profiles;
}

export function getActiveProfile(): DaemonProfile {
  const { profiles, activeProfileId } = ensureProfilesInitialized();
  return profiles.find((p) => p.id === activeProfileId) || profiles[0];
}

export function setActiveProfile(profileId: string): void {
  const { profiles } = ensureProfilesInitialized();
  if (!profiles.some((p) => p.id === profileId)) {
    throw new Error("profile not found");
  }
  localStorage.setItem(ACTIVE_PROFILE_ID_KEY, profileId);
  syncLegacyWithActiveProfile();
}

export function upsertProfile(
  profile: Pick<DaemonProfile, "label" | "baseUrl" | "token"> & Partial<Pick<DaemonProfile, "id" | "deviceId" | "lastSeenAt">>,
  options?: { setActive?: boolean }
): DaemonProfile {
  const normalized = normalizeProfile(profile);
  const current = ensureProfilesInitialized();

  let next: DaemonProfile;
  const idx = normalized.id ? current.profiles.findIndex((p) => p.id === normalized.id) : -1;
  if (idx >= 0) {
    next = { ...current.profiles[idx], ...normalized };
    current.profiles[idx] = next;
  } else {
    next = {
      id: normalized.id || newProfileId(),
      label: normalized.label,
      baseUrl: normalized.baseUrl,
      token: normalized.token,
      deviceId: normalized.deviceId ?? null,
      lastSeenAt: normalized.lastSeenAt ?? null,
    };
    current.profiles.unshift(next);
  }

  persistProfiles(current.profiles);
  if (options?.setActive || !current.activeProfileId) {
    localStorage.setItem(ACTIVE_PROFILE_ID_KEY, next.id);
  }
  syncLegacyWithActiveProfile();
  return next;
}

export function deleteProfile(profileId: string): void {
  const current = ensureProfilesInitialized();
  let nextProfiles = current.profiles.filter((p) => p.id !== profileId);

  if (nextProfiles.length === 0) {
    nextProfiles = [defaultProfile()];
  }

  persistProfiles(nextProfiles);
  const activeId = localStorage.getItem(ACTIVE_PROFILE_ID_KEY) || "";
  if (!nextProfiles.some((p) => p.id === activeId)) {
    localStorage.setItem(ACTIVE_PROFILE_ID_KEY, nextProfiles[0].id);
  }
  syncLegacyWithActiveProfile();
}

export function loadApiConfig(): ApiConfig {
  const active = getActiveProfile();
  return { baseUrl: active.baseUrl, token: active.token };
}

export function saveApiConfig(cfg: ApiConfig): void {
  const active = getActiveProfile();
  upsertProfile(
    {
      id: active.id,
      label: active.label,
      baseUrl: cfg.baseUrl,
      token: cfg.token,
      deviceId: active.deviceId ?? undefined,
      lastSeenAt: active.lastSeenAt ?? undefined,
    },
    { setActive: true }
  );
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

export async function getSettings(): Promise<DaemonSettings> {
  return apiFetch("/api/v1/settings");
}

export async function updateSettings(payload: UpdateSettingsRequest): Promise<DaemonSettings> {
  return apiFetch("/api/v1/settings", {
    method: "PUT",
    body: JSON.stringify(payload),
  });
}

export async function rotateToken(): Promise<RotateTokenResponse> {
  return apiFetch("/api/v1/auth/token/rotate", { method: "POST" });
}

export async function startPair(payload: PairStartRequest): Promise<PairStartResponse> {
  return apiFetch("/api/v1/auth/pair/start", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function completePair(payload: PairCompleteRequest): Promise<PairCompleteResponse> {
  return apiFetch("/api/v1/auth/pair/complete", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function getPairStatus(pairId: string, secret: string): Promise<PairStatusResponse> {
  return apiFetch(`/api/v1/auth/pair/status/${encodeURIComponent(pairId)}?secret=${encodeURIComponent(secret)}`);
}

export async function listPendingPairs(): Promise<PendingPairSessionsResponse> {
  return apiFetch("/api/v1/auth/pair/pending");
}

export async function decidePair(payload: PairDecisionRequest): Promise<PairDecisionResponse> {
  return apiFetch("/api/v1/auth/pair/decision", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function listAuthDevices(): Promise<AuthDeviceListResponse> {
  return apiFetch("/api/v1/auth/devices");
}

export async function revokeAuthDevice(deviceId: string): Promise<void> {
  await apiFetch(`/api/v1/auth/devices/${encodeURIComponent(deviceId)}`, { method: "DELETE" });
}

export function isDesktopRuntime(): boolean {
  return typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__?.invoke === "function";
}

export async function daemonStatusDesktop(): Promise<DaemonStatus> {
  return invokeDesktop("daemon_status") as Promise<DaemonStatus>;
}

export async function daemonStartDesktop(): Promise<DaemonActionResponse> {
  return invokeDesktop("daemon_start") as Promise<DaemonActionResponse>;
}

export async function daemonStopDesktop(): Promise<DaemonActionResponse> {
  return invokeDesktop("daemon_stop") as Promise<DaemonActionResponse>;
}

export async function daemonRestartDesktop(): Promise<DaemonActionResponse> {
  return invokeDesktop("daemon_restart") as Promise<DaemonActionResponse>;
}

export async function daemonBootstrapDesktop(): Promise<DaemonActionResponse> {
  return invokeDesktop("daemon_bootstrap") as Promise<DaemonActionResponse>;
}

async function invokeDesktop(command: string, args?: Record<string, unknown>): Promise<unknown> {
  const invoker = window.__TAURI_INTERNALS__?.invoke;
  if (!invoker) {
    throw new Error("Desktop runtime is not available.");
  }
  return invoker(command, args);
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

function ensureProfilesInitialized(): { profiles: DaemonProfile[]; activeProfileId: string } {
  const parsedProfiles = parseProfiles(localStorage.getItem(PROFILES_KEY));
  if (parsedProfiles.length > 0) {
    let active = localStorage.getItem(ACTIVE_PROFILE_ID_KEY) || "";
    if (!parsedProfiles.some((p) => p.id === active)) {
      active = parsedProfiles[0].id;
      localStorage.setItem(ACTIVE_PROFILE_ID_KEY, active);
    }
    syncLegacyWithProfile(parsedProfiles.find((p) => p.id === active) || parsedProfiles[0]);
    return { profiles: parsedProfiles, activeProfileId: active };
  }

  const legacyBase = localStorage.getItem(LEGACY_BASE_URL_KEY);
  const legacyToken = localStorage.getItem(LEGACY_TOKEN_KEY);
  const initial = normalizeProfile({
    id: newProfileId(),
    label: "Local Host",
    baseUrl: legacyBase || resolveDefaultBaseUrl(typeof window !== "undefined" ? window.location : undefined),
    token: legacyToken || "",
  });
  const profile: DaemonProfile = {
    id: initial.id || newProfileId(),
    label: initial.label,
    baseUrl: initial.baseUrl,
    token: initial.token,
    deviceId: null,
    lastSeenAt: null,
  };
  persistProfiles([profile]);
  localStorage.setItem(ACTIVE_PROFILE_ID_KEY, profile.id);
  syncLegacyWithProfile(profile);
  return { profiles: [profile], activeProfileId: profile.id };
}

function parseProfiles(raw: string | null): DaemonProfile[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    const out: DaemonProfile[] = [];
    for (const item of parsed) {
      const normalized = normalizeProfile(item as Partial<DaemonProfile>);
      const id = normalized.id || newProfileId();
      out.push({
        id,
        label: normalized.label,
        baseUrl: normalized.baseUrl,
        token: normalized.token,
        deviceId: normalized.deviceId,
        lastSeenAt: normalized.lastSeenAt,
      });
    }
    return out;
  } catch {
    return [];
  }
}

function persistProfiles(profiles: DaemonProfile[]) {
  localStorage.setItem(PROFILES_KEY, JSON.stringify(profiles));
}

function syncLegacyWithActiveProfile() {
  const active = getActiveProfile();
  syncLegacyWithProfile(active);
}

function syncLegacyWithProfile(profile: DaemonProfile) {
  localStorage.setItem(LEGACY_BASE_URL_KEY, profile.baseUrl);
  localStorage.setItem(LEGACY_TOKEN_KEY, profile.token);
}

function normalizeProfile(profile: Partial<DaemonProfile>): {
  id: string;
  label: string;
  baseUrl: string;
  token: string;
  deviceId: string | null;
  lastSeenAt: string | null;
} {
  return {
    id: profile.id?.trim() || "",
    label: (profile.label || "Daemon").trim(),
    baseUrl: (profile.baseUrl || FALLBACK_DAEMON_BASE_URL).trim().replace(/\/$/, ""),
    token: (profile.token || "").trim(),
    deviceId: profile.deviceId ?? null,
    lastSeenAt: profile.lastSeenAt ?? null,
  };
}

function defaultProfile(): DaemonProfile {
  return {
    id: newProfileId(),
    label: "Local Host",
    baseUrl: resolveDefaultBaseUrl(typeof window !== "undefined" ? window.location : undefined),
    token: "",
    deviceId: null,
    lastSeenAt: null,
  };
}

function newProfileId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `profile-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
