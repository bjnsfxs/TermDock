import React, { useEffect, useMemo, useRef, useState } from "react";
import QRCode from "qrcode";
import {
  daemonBootstrapDesktop,
  daemonRestartDesktop,
  daemonStartDesktop,
  daemonStatusDesktop,
  daemonStopDesktop,
  decidePair,
  deleteProfile,
  getActiveProfile,
  getSettings,
  isDesktopRuntime,
  listAuthDevices,
  listPendingPairs,
  listProfiles,
  revokeAuthDevice,
  rotateToken,
  saveApiConfig,
  setActiveProfile,
  startPair,
  updateSettings,
  upsertProfile,
} from "../lib/api";
import type { AuthDevice, DaemonProfile, DaemonStatus, PendingPairSession, PairStartResponse } from "../lib/types";
import { validatePortInput } from "./settings-utils";

type Notice = {
  kind: "success" | "error" | "info";
  text: string;
} | null;

function alertClass(kind: NonNullable<Notice>["kind"]): string {
  if (kind === "success") return "alert success";
  if (kind === "error") return "alert error";
  return "alert info";
}

function formatExpiry(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

export default function Settings() {
  const initialProfile = getActiveProfile();
  const [profiles, setProfiles] = useState<DaemonProfile[]>(() => listProfiles());
  const [activeProfileId, setActiveProfileId] = useState(initialProfile.id);
  const [profileLabel, setProfileLabel] = useState(initialProfile.label);
  const [baseUrl, setBaseUrl] = useState(initialProfile.baseUrl);
  const [token, setToken] = useState(initialProfile.token);
  const [bindAddress, setBindAddress] = useState("127.0.0.1");
  const [daemonPort, setDaemonPort] = useState("8765");
  const [pairSession, setPairSession] = useState<PairStartResponse | null>(null);
  const [pairQrDataUrl, setPairQrDataUrl] = useState<string | null>(null);
  const [pendingPairs, setPendingPairs] = useState<PendingPairSession[]>([]);
  const [devices, setDevices] = useState<AuthDevice[]>([]);
  const [notice, setNotice] = useState<Notice>(null);
  const [daemonStatus, setDaemonStatus] = useState<DaemonStatus | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);
  const [loadingSettings, setLoadingSettings] = useState(false);
  const [applyingSettings, setApplyingSettings] = useState(false);
  const [rotatingToken, setRotatingToken] = useState(false);
  const [loadingPair, setLoadingPair] = useState(false);
  const [pairActionLoading, setPairActionLoading] = useState(false);
  const [daemonControlLoading, setDaemonControlLoading] = useState(false);
  const desktopRuntime = isDesktopRuntime();
  const noticeTimerRef = useRef<number | null>(null);

  const lanWarning = useMemo(() => {
    try {
      const hostname = new URL(baseUrl).hostname;
      if (hostname === "127.0.0.1" || hostname === "localhost" || hostname === "::1") {
        return "Current base URL is loopback-only. Mobile pairing cannot reach this address.";
      }
      return null;
    } catch {
      return "Base URL is invalid. Pairing QR may not be usable.";
    }
  }, [baseUrl]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setCheckingHealth(true);
      try {
        const url = baseUrl.trim().replace(/\/$/, "") + "/health";
        const res = await fetch(url);
        if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
        if (!cancelled) {
          if (!desktopRuntime) {
            setDaemonStatus({ reachable: true, managed: false, pid: null, baseUrl: baseUrl.trim(), message: null });
          }
        }
      } catch {
        if (!cancelled && !desktopRuntime) {
          setDaemonStatus({ reachable: false, managed: false, pid: null, baseUrl: baseUrl.trim(), message: null });
        }
      } finally {
        if (!cancelled) setCheckingHealth(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [baseUrl, desktopRuntime]);

  useEffect(() => {
    let cancelled = false;
    if (!pairSession?.pair_uri) {
      setPairQrDataUrl(null);
      return () => {
        cancelled = true;
      };
    }

    QRCode.toDataURL(pairSession.pair_uri, { width: 220, margin: 1, errorCorrectionLevel: "M" })
      .then((dataUrl) => {
        if (!cancelled) setPairQrDataUrl(dataUrl);
      })
      .catch(() => {
        if (!cancelled) setPairQrDataUrl(null);
      });

    return () => {
      cancelled = true;
    };
  }, [pairSession]);

  useEffect(() => {
    return () => {
      if (noticeTimerRef.current !== null) {
        window.clearTimeout(noticeTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!desktopRuntime) return;
    let cancelled = false;

    const tick = async () => {
      try {
        const status = await daemonStatusDesktop();
        if (!cancelled) setDaemonStatus(status);
      } catch (err) {
        if (!cancelled) {
          flash("error", (err as Error).message, 2500);
        }
      }
    };

    void tick();
    const timer = window.setInterval(() => {
      void tick();
    }, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [desktopRuntime]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const [pending, list] = await Promise.all([listPendingPairs(), listAuthDevices()]);
        if (cancelled) return;
        setPendingPairs(pending.sessions);
        setDevices(list.devices);
      } catch {
        // ignore noisy polling errors; button actions still surface explicit errors.
      }
    };
    void tick();
    const timer = window.setInterval(() => {
      void tick();
    }, 4000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  function flash(kind: "success" | "error" | "info", text: string, timeoutMs = 2000): void {
    if (noticeTimerRef.current !== null) {
      window.clearTimeout(noticeTimerRef.current);
      noticeTimerRef.current = null;
    }
    setNotice({ kind, text });
    noticeTimerRef.current = window.setTimeout(() => {
      setNotice((prev) => (prev?.text === text ? null : prev));
      noticeTimerRef.current = null;
    }, timeoutMs);
  }

  function reloadProfiles() {
    const nextProfiles = listProfiles();
    const active = getActiveProfile();
    setProfiles(nextProfiles);
    setActiveProfileId(active.id);
    setProfileLabel(active.label);
    setBaseUrl(active.baseUrl);
    setToken(active.token);
  }

  function persistLocalConfig(next?: { baseUrl?: string; token?: string; label?: string }) {
    const cfg = {
      baseUrl: (next?.baseUrl ?? baseUrl).trim(),
      token: (next?.token ?? token).trim(),
      label: (next?.label ?? profileLabel).trim() || "Daemon",
    };
    upsertProfile(
      {
        id: activeProfileId,
        label: cfg.label,
        baseUrl: cfg.baseUrl,
        token: cfg.token,
      },
      { setActive: true }
    );
    saveApiConfig({ baseUrl: cfg.baseUrl, token: cfg.token });
    return cfg;
  }

  async function onLoadSettings() {
    setLoadingSettings(true);
    try {
      const cfg = persistLocalConfig();
      const s = await getSettings();
      setBindAddress(s.bind_address);
      setDaemonPort(String(s.port));
      setToken(s.token);
      upsertProfile(
        {
          id: activeProfileId,
          label: cfg.label,
          baseUrl: cfg.baseUrl,
          token: s.token,
        },
        { setActive: true }
      );
      reloadProfiles();
      flash("success", `Daemon settings loaded: ${s.bind_address}:${s.port}`);
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    } finally {
      setLoadingSettings(false);
    }
  }

  async function onApplyDaemonSettings() {
    setApplyingSettings(true);
    try {
      const trimmedBind = bindAddress.trim();
      if (!trimmedBind) {
        flash("error", "bind_address cannot be empty.");
        return;
      }
      const portValidation = validatePortInput(daemonPort);
      if (!portValidation.ok) {
        flash("error", portValidation.message);
        return;
      }

      persistLocalConfig();
      const s = await updateSettings({ bind_address: trimmedBind, port: portValidation.port });
      setBindAddress(s.bind_address);
      setDaemonPort(String(s.port));
      flash("info", "Daemon settings updated. Restart daemon for bind/port changes to take effect.", 2600);
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    } finally {
      setApplyingSettings(false);
    }
  }

  async function onRotateToken() {
    setRotatingToken(true);
    try {
      const cfg = persistLocalConfig();
      const res = await rotateToken();
      setToken(res.token);
      upsertProfile(
        {
          id: activeProfileId,
          label: cfg.label,
          baseUrl: cfg.baseUrl,
          token: res.token,
        },
        { setActive: true }
      );
      reloadProfiles();
      flash("success", "Token rotated and saved.");
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    } finally {
      setRotatingToken(false);
    }
  }

  async function onDaemonAction(kind: "bootstrap" | "start" | "stop" | "restart") {
    if (!desktopRuntime) return;
    setDaemonControlLoading(true);
    try {
      const response =
        kind === "bootstrap"
          ? await daemonBootstrapDesktop()
          : kind === "start"
            ? await daemonStartDesktop()
            : kind === "stop"
              ? await daemonStopDesktop()
              : await daemonRestartDesktop();
      setDaemonStatus(response.status);
      flash("success", response.status.message || `Daemon ${kind} completed.`);
    } catch (e) {
      flash("error", (e as Error).message, 3000);
    } finally {
      setDaemonControlLoading(false);
    }
  }

  async function onCreatePairSession() {
    setLoadingPair(true);
    try {
      persistLocalConfig();
      const session = await startPair({ base_url: baseUrl.trim(), ttl_seconds: 120 });
      setPairSession(session);
      flash("success", "Pairing session created.");
    } catch (e) {
      flash("error", (e as Error).message, 3200);
    } finally {
      setLoadingPair(false);
    }
  }

  async function onPairDecision(pairId: string, decision: "approve" | "reject") {
    setPairActionLoading(true);
    try {
      await decidePair({ pair_id: pairId, decision });
      const pending = await listPendingPairs();
      setPendingPairs(pending.sessions);
      flash("success", decision === "approve" ? "Device approved." : "Pair request rejected.");
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    } finally {
      setPairActionLoading(false);
    }
  }

  async function onRevokeDevice(deviceId: string) {
    try {
      await revokeAuthDevice(deviceId);
      const list = await listAuthDevices();
      setDevices(list.devices);
      flash("info", "Device revoked.");
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    }
  }

  async function copyText(value: string, label: string) {
    if (!value) {
      flash("error", `${label} is empty.`);
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      flash("success", `${label} copied.`);
      return;
    } catch {
      const el = document.createElement("textarea");
      el.value = value;
      el.style.position = "fixed";
      el.style.left = "-9999px";
      document.body.appendChild(el);
      el.select();
      document.execCommand("copy");
      document.body.removeChild(el);
      flash("success", `${label} copied.`);
    }
  }

  return (
    <section>
      <header className="page-header">
        <h1 className="page-title">Settings</h1>
        <p className="page-subtitle">Manage daemon endpoint, desktop lifecycle, and mobile pairing approvals.</p>
      </header>

      {notice && (
        <div className={alertClass(notice.kind)} role="status" aria-live="polite">
          {notice.text}
        </div>
      )}

      <div className="settings-grid space-top">
        <div className="stack">
          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Connection Profiles</h2>
            </div>
            <div className="card-content stack">
              <label className="field">
                <span className="field-label">Active Profile</span>
                <select
                  className="text-input"
                  value={activeProfileId}
                  onChange={(e) => {
                    setActiveProfile(e.target.value);
                    reloadProfiles();
                  }}
                >
                  {profiles.map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span className="field-label">Profile Label</span>
                <input className="text-input" value={profileLabel} onChange={(e) => setProfileLabel(e.target.value)} />
              </label>
              <label className="field">
                <span className="field-label">Daemon Base URL</span>
                <input className="text-input mono-text" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
              </label>
              <div className="field">
                <span className="field-label">Token</span>
                <div className="token-row">
                  <input className="text-input mono-text" value={token} onChange={(e) => setToken(e.target.value)} />
                  <div className="token-actions">
                    <button className="btn btn-secondary" type="button" onClick={() => void copyText(token, "Token")} disabled={!token.trim()}>
                      Copy
                    </button>
                    <button className="btn btn-secondary" type="button" onClick={() => void onRotateToken()} disabled={rotatingToken}>
                      {rotatingToken ? "Rotating..." : "Rotate"}
                    </button>
                  </div>
                </div>
              </div>
              <div className="btn-row">
                <button
                  className="btn btn-secondary"
                  type="button"
                  onClick={() => {
                    persistLocalConfig();
                    reloadProfiles();
                    flash("success", "Profile saved.");
                  }}
                >
                  Save Profile
                </button>
                <button
                  className="btn btn-secondary"
                  type="button"
                  onClick={() => {
                    upsertProfile({ label: profileLabel || "New Profile", baseUrl, token }, { setActive: true });
                    reloadProfiles();
                    flash("success", "New profile created.");
                  }}
                >
                  Save as New
                </button>
                <button
                  className="btn btn-danger"
                  type="button"
                  onClick={() => {
                    if (!window.confirm("Delete this profile?")) return;
                    deleteProfile(activeProfileId);
                    reloadProfiles();
                    flash("info", "Profile deleted.");
                  }}
                >
                  Delete Profile
                </button>
              </div>
            </div>
          </section>

          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Daemon Runtime</h2>
            </div>
            <div className="card-content stack">
              <div className="health-row">
                <span className={`status-badge ${daemonStatus?.reachable ? "status-running" : "status-stopped"}`}>
                  {daemonStatus?.reachable ? "running" : "stopped"}
                </span>
                <span className="health-pill">managed: {daemonStatus?.managed ? "yes" : "no"}</span>
                <span className="health-pill">pid: {daemonStatus?.pid ?? "-"}</span>
                <span className="health-pill">health: {checkingHealth ? "checking..." : daemonStatus?.reachable ? "ok" : "down"}</span>
              </div>

              {desktopRuntime && (
                <div className="btn-row">
                  <button className="btn btn-primary" type="button" onClick={() => void onDaemonAction("bootstrap")} disabled={daemonControlLoading}>
                    Bootstrap
                  </button>
                  <button className="btn btn-success" type="button" onClick={() => void onDaemonAction("start")} disabled={daemonControlLoading}>
                    Start
                  </button>
                  <button className="btn btn-danger" type="button" onClick={() => void onDaemonAction("stop")} disabled={daemonControlLoading}>
                    Stop
                  </button>
                  <button className="btn btn-warning" type="button" onClick={() => void onDaemonAction("restart")} disabled={daemonControlLoading}>
                    Restart
                  </button>
                </div>
              )}

              <div className="grid-2">
                <label className="field">
                  <span className="field-label">Bind Address</span>
                  <input
                    className="text-input mono-text"
                    value={bindAddress}
                    onChange={(e) => setBindAddress(e.target.value)}
                    placeholder="127.0.0.1 / 0.0.0.0 / LAN IP"
                  />
                </label>
                <label className="field">
                  <span className="field-label">Port</span>
                  <input className="text-input mono-text" value={daemonPort} onChange={(e) => setDaemonPort(e.target.value)} placeholder="8765" />
                </label>
              </div>

              <div className="notice">Changing bind address requires daemon restart after apply.</div>
              <div className="btn-row">
                <button className="btn btn-secondary" type="button" onClick={() => void onLoadSettings()} disabled={loadingSettings}>
                  {loadingSettings ? "Fetching..." : "Fetch Settings"}
                </button>
                <button className="btn btn-primary" type="button" onClick={() => void onApplyDaemonSettings()} disabled={applyingSettings}>
                  {applyingSettings ? "Applying..." : "Apply Daemon Settings"}
                </button>
              </div>
            </div>
          </section>
        </div>

        <div className="stack">
          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Mobile Pairing</h2>
            </div>
            <div className="card-content stack">
              <div className="btn-row">
                <button className="btn btn-primary" type="button" onClick={() => void onCreatePairSession()} disabled={loadingPair}>
                  {loadingPair ? "Generating..." : "Create Pair QR"}
                </button>
                <button className="btn btn-secondary" type="button" onClick={() => void copyText(pairSession?.pair_uri || "", "Pair URI")}>
                  Copy URI
                </button>
              </div>
              {pairSession && (
                <>
                  <div className="small-text">
                    Pair ID: <code>{pairSession.pair_id}</code>
                  </div>
                  <div className="small-text">Expires: {formatExpiry(pairSession.expires_at_epoch)}</div>
                  <div className="pairing-qr">{pairQrDataUrl ? <img src={pairQrDataUrl} alt="pairing qr" width={220} height={220} /> : null}</div>
                  <div className="pairing-box mono-text">{pairSession.pair_uri}</div>
                </>
              )}
              {lanWarning && <div className="notice">{lanWarning}</div>}
            </div>
          </section>

          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Pending Approvals</h2>
            </div>
            <div className="card-content stack">
              {pendingPairs.length === 0 ? (
                <div className="small-text">No pending pair requests.</div>
              ) : (
                pendingPairs.map((session) => (
                  <div key={session.pair_id} className="pairing-box">
                    <div className="small-text mono-text">{session.pair_id}</div>
                    <div className="small-text">
                      Device: {session.requested_name || "-"} ({session.platform || "unknown"})
                    </div>
                    <div className="small-text">Expires: {formatExpiry(session.expires_at_epoch)}</div>
                    <div className="btn-row">
                      <button
                        className="btn btn-success"
                        type="button"
                        onClick={() => void onPairDecision(session.pair_id, "approve")}
                        disabled={pairActionLoading}
                      >
                        Approve
                      </button>
                      <button
                        className="btn btn-danger"
                        type="button"
                        onClick={() => void onPairDecision(session.pair_id, "reject")}
                        disabled={pairActionLoading}
                      >
                        Reject
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </section>

          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Trusted Devices</h2>
            </div>
            <div className="card-content stack">
              {devices.length === 0 ? (
                <div className="small-text">No trusted devices.</div>
              ) : (
                devices.map((device) => (
                  <div key={device.id} className="pairing-box">
                    <div className="small-text mono-text">{device.id}</div>
                    <div className="small-text">{device.name}</div>
                    <div className="small-text">Last seen: {device.last_seen_at || "-"}</div>
                    <div className="btn-row">
                      <button className="btn btn-danger" type="button" onClick={() => void onRevokeDevice(device.id)}>
                        Revoke
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}
