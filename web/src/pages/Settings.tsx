import React, { useEffect, useMemo, useState } from "react";
import QRCode from "qrcode";
import {
  getSettings,
  loadApiConfig,
  rotateToken,
  saveApiConfig,
  updateSettings,
} from "../lib/api";

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
  const [daemon, setDaemon] = useState<any>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const url = baseUrl.trim().replace(/\/$/, "") + "/health";
        const res = await fetch(url);
        if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
        if (!cancelled) setDaemon(await res.json());
      } catch {
        if (!cancelled) setDaemon(null);
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

  function flash(kind: "success" | "error" | "info", text: string, timeoutMs = 2000) {
    setNotice({ kind, text });
    window.setTimeout(() => setNotice((prev) => (prev?.text === text ? null : prev)), timeoutMs);
  }

  async function onSave() {
    saveApiConfig({ baseUrl, token });
    flash("success", "Saved local base URL and token.");
  }

  async function onLoadSettings() {
    try {
      saveApiConfig({ baseUrl, token });
      const s = await getSettings();
      setBindAddress(s.bind_address);
      setDaemonPort(String(s.port));
      setToken(s.token);
      saveApiConfig({ baseUrl, token: s.token });
      flash("success", `Daemon settings loaded: ${s.bind_address}:${s.port}`);
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    }
  }

  async function onApplyDaemonSettings() {
    try {
      const trimmedBind = bindAddress.trim();
      if (!trimmedBind) {
        flash("error", "bind_address cannot be empty.");
        return;
      }
      const parsedPort = Number(daemonPort);
      if (!Number.isInteger(parsedPort) || parsedPort <= 0 || parsedPort > 65535) {
        flash("error", "port must be an integer in [1, 65535].");
        return;
      }

      saveApiConfig({ baseUrl, token });
      const s = await updateSettings({ bind_address: trimmedBind, port: parsedPort });
      setBindAddress(s.bind_address);
      setDaemonPort(String(s.port));
      flash("info", "Daemon settings updated. Restart daemon for bind/port changes to take effect.", 2600);
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    }
  }

  async function onRotateToken() {
    try {
      saveApiConfig({ baseUrl, token });
      const res = await rotateToken();
      setToken(res.token);
      saveApiConfig({ baseUrl, token: res.token });
      flash("success", "Token rotated and saved.");
    } catch (e) {
      flash("error", (e as Error).message, 2800);
    }
  }

  async function copyText(value: string, label: string) {
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

      {notice && <div className={alertClass(notice.kind)}>{notice.text}</div>}

      <div className="settings-grid space-top">
        <div className="stack">
          <section className="surface-card">
            <div className="card-header">
              <h2 className="card-title">General Settings</h2>
            </div>
            <div className="card-content stack">
              <label className="field">
                <span className="field-label">Daemon Base URL</span>
                <input className="text-input mono-text" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
              </label>

              <div className="field">
                <span className="field-label">Token</span>
                <div className="token-row">
                  <input className="text-input mono-text" value={token} onChange={(e) => setToken(e.target.value)} />
                  <div className="token-actions">
                    <button className="btn btn-secondary" onClick={() => void copyText(token, "Token")}>
                      Copy
                    </button>
                    <button className="btn btn-secondary" onClick={() => void onRotateToken()}>
                      Rotate
                    </button>
                  </div>
                </div>
              </div>

              <div className="btn-row">
                <button className="btn btn-secondary" onClick={() => void onSave()}>
                  Save Local
                </button>
              </div>

              <div className="small-text">
                Health: <span className="mono-text">{daemon ? JSON.stringify(daemon) : "(not reachable)"}</span>
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
                  />
                </label>
                <label className="field">
                  <span className="field-label">Port</span>
                  <input className="text-input mono-text" value={daemonPort} onChange={(e) => setDaemonPort(e.target.value)} placeholder="8765" />
                </label>
              </div>

              <div className="notice">Changing bind address requires daemon restart manually after apply.</div>

              <div className="btn-row">
                <button className="btn btn-secondary" onClick={() => void onLoadSettings()}>
                  Fetch Settings
                </button>
                <button className="btn btn-primary" onClick={() => void onApplyDaemonSettings()}>
                  Apply Daemon Settings
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
              <button className="btn btn-primary" onClick={() => void copyText(pairingUri, "Pairing URI")} disabled={!pairingUri}>
                Copy Pairing URI
              </button>
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}
