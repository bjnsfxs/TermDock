import React, { useEffect, useMemo, useRef, useState } from "react";
import QRCode from "qrcode";
import {
  getSettings,
  loadApiConfig,
  rotateToken,
  saveApiConfig,
  updateSettings,
} from "../lib/api";
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

export default function Settings() {
  const initialConfig = loadApiConfig();
  const [baseUrl, setBaseUrl] = useState(initialConfig.baseUrl);
  const [token, setToken] = useState(initialConfig.token);
  const [bindAddress, setBindAddress] = useState("127.0.0.1");
  const [daemonPort, setDaemonPort] = useState("8765");
  const [pairQrDataUrl, setPairQrDataUrl] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [daemon, setDaemon] = useState<Record<string, unknown> | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);
  const [loadingSettings, setLoadingSettings] = useState(false);
  const [applyingSettings, setApplyingSettings] = useState(false);
  const [rotatingToken, setRotatingToken] = useState(false);
  const noticeTimerRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setCheckingHealth(true);
      try {
        const url = baseUrl.trim().replace(/\/$/, "") + "/health";
        const res = await fetch(url);
        if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
        if (!cancelled) setDaemon(await res.json());
      } catch {
        if (!cancelled) setDaemon(null);
      } finally {
        if (!cancelled) setCheckingHealth(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  const pairingUri = useMemo(() => {
    const normalizedBase = baseUrl.trim();
    const normalizedToken = token.trim();
    if (!normalizedBase || !normalizedToken) return "";
    return `aicli-manager://pair?baseUrl=${encodeURIComponent(normalizedBase)}&token=${encodeURIComponent(normalizedToken)}`;
  }, [baseUrl, token]);

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
    if (!pairingUri) {
      setPairQrDataUrl(null);
      return () => {
        cancelled = true;
      };
    }

    QRCode.toDataURL(pairingUri, { width: 220, margin: 1, errorCorrectionLevel: "M" })
      .then((dataUrl) => {
        if (!cancelled) setPairQrDataUrl(dataUrl);
      })
      .catch(() => {
        if (!cancelled) setPairQrDataUrl(null);
      });

    return () => {
      cancelled = true;
    };
  }, [pairingUri]);

  useEffect(() => {
    return () => {
      if (noticeTimerRef.current !== null) {
        window.clearTimeout(noticeTimerRef.current);
      }
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

  function persistLocalConfig(next?: { baseUrl?: string; token?: string }): { baseUrl: string; token: string } {
    const cfg = {
      baseUrl: (next?.baseUrl ?? baseUrl).trim(),
      token: (next?.token ?? token).trim(),
    };
    saveApiConfig(cfg);
    return cfg;
  }

  function onSave() {
    const cfg = persistLocalConfig();
    setBaseUrl(cfg.baseUrl);
    setToken(cfg.token);
    flash("success", "Saved local base URL and token.");
  }

  async function onLoadSettings() {
    setLoadingSettings(true);
    try {
      const cfg = persistLocalConfig();
      const s = await getSettings();
      setBindAddress(s.bind_address);
      setDaemonPort(String(s.port));
      setToken(s.token);
      saveApiConfig({ baseUrl: cfg.baseUrl, token: s.token });
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
      saveApiConfig({ baseUrl: cfg.baseUrl, token: res.token });
      flash("success", "Token rotated and saved.");
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    } finally {
      setRotatingToken(false);
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
        <p className="page-subtitle">Manage daemon endpoint, token lifecycle, and mobile pairing.</p>
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
              <h2 className="card-title">General Settings</h2>
            </div>
            <div className="card-content stack">
              <label className="field">
                <span className="field-label">Daemon Base URL</span>
                <input
                  className="text-input mono-text"
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  aria-label="Daemon base URL"
                />
              </label>

              <div className="field">
                <span className="field-label">Token</span>
                <div className="token-row">
                  <input
                    className="text-input mono-text"
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                    aria-label="Daemon token"
                  />
                  <div className="token-actions">
                    <button
                      className="btn btn-secondary"
                      type="button"
                      onClick={() => void copyText(token, "Token")}
                      disabled={!token.trim() || rotatingToken}
                    >
                      Copy
                    </button>
                    <button className="btn btn-secondary" type="button" onClick={() => void onRotateToken()} disabled={rotatingToken}>
                      {rotatingToken ? "Rotating..." : "Rotate"}
                    </button>
                  </div>
                </div>
              </div>

              <div className="btn-row">
                <button className="btn btn-secondary" type="button" onClick={() => onSave()}>
                  Save Local
                </button>
              </div>

              <div className="small-text">
                Health:{" "}
                <span className="mono-text">
                  {checkingHealth ? "(checking...)" : daemon ? JSON.stringify(daemon) : "(not reachable)"}
                </span>
              </div>
            </div>
          </section>

          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">Daemon Runtime</h2>
            </div>
            <div className="card-content stack">
              <div className="grid-2">
                <label className="field">
                  <span className="field-label">Bind Address</span>
                  <input
                    className="text-input mono-text"
                    value={bindAddress}
                    onChange={(e) => setBindAddress(e.target.value)}
                    placeholder="127.0.0.1 / 0.0.0.0 / LAN IP"
                    aria-label="Daemon bind address"
                  />
                </label>
                <label className="field">
                  <span className="field-label">Port</span>
                  <input
                    className="text-input mono-text"
                    value={daemonPort}
                    onChange={(e) => setDaemonPort(e.target.value)}
                    placeholder="8765"
                    aria-label="Daemon port"
                  />
                </label>
              </div>

              <div className="notice">Changing bind address requires daemon restart manually after apply.</div>

              <div className="btn-row">
                <button className="btn btn-secondary" type="button" onClick={() => void onLoadSettings()} disabled={loadingSettings}>
                  {loadingSettings ? "Fetching..." : "Fetch Settings"}
                </button>
                <button className="btn btn-primary" type="button" onClick={() => void onApplyDaemonSettings()} disabled={applyingSettings}>
                  {applyingSettings ? "Applying..." : "Apply Daemon Settings"}
                </button>
              </div>

              <div className="small-text">
                REST uses <code>Authorization: Bearer</code>. Browser WebSocket can use query token fallback.
              </div>
            </div>
          </section>
        </div>

        <section className="surface-card">
          <div className="card-header">
            <h2 className="card-title">Mobile Pairing</h2>
          </div>
          <div className="card-content stack" style={{ height: "100%" }}>
            <div className="pairing-qr">
              {pairQrDataUrl ? (
                <img src={pairQrDataUrl} alt="pairing qr" width={220} height={220} />
              ) : (
                <span className="small-text">QR unavailable</span>
              )}
            </div>

            <div className="small-text" style={{ textAlign: "center" }}>
              Scan with mobile app or copy URI directly.
            </div>

            <div className="pairing-box mono-text">{pairingUri || "(fill base URL and token first)"}</div>

            {lanWarning && <div className="notice">{lanWarning}</div>}

            <div className="btn-row">
              <button
                className="btn btn-primary"
                type="button"
                onClick={() => void copyText(pairingUri, "Pairing URI")}
                disabled={!pairingUri}
              >
                Copy Pairing URI
              </button>
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}
