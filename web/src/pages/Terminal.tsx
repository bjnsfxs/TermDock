import React, { useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import "xterm/css/xterm.css";
import { buildWsUrl } from "../lib/api";
import type { TerminalControlMessage } from "../lib/types";

type SessionStatus = "connecting" | "connected" | "disconnected";

function statusBadgeClass(status: SessionStatus): string {
  if (status === "connected") return "status-badge status-running";
  if (status === "connecting") return "status-badge status-generic";
  return "status-badge status-stopped";
}

export default function TerminalPage() {
  const { id } = useParams();
  const ref = useRef<HTMLDivElement | null>(null);
  const fit = useMemo(() => new FitAddon(), []);
  const [err, setErr] = useState<string | null>(null);
  const [sessionStatus, setSessionStatus] = useState<SessionStatus>("connecting");
  const [backend, setBackend] = useState<string>("-");
  const [clients, setClients] = useState<number>(0);
  const [sessionNonce, setSessionNonce] = useState(0);

  useEffect(() => {
    if (!id || !ref.current) return;

    const term = new Terminal({
      cursorBlink: true,
      convertEol: false,
      theme: {
        background: "#05070b",
      },
    });
    term.loadAddon(fit);
    term.open(ref.current);
    fit.fit();

    const decoder = new TextDecoder();
    const encoder = new TextEncoder();

    const wsUrl = buildWsUrl(`/ws/v1/term/${id}`);
    const ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";

    setSessionStatus("connecting");
    setErr(null);
    setBackend("-");
    setClients(0);

    const sendJson = (payload: unknown) => {
      if (ws.readyState !== WebSocket.OPEN) return;
      ws.send(JSON.stringify(payload));
    };

    const sendResize = () => {
      fit.fit();
      sendJson({ type: "resize", cols: term.cols, rows: term.rows });
    };

    let resizeTimer: number | null = null;
    const onResize = () => {
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      resizeTimer = window.setTimeout(() => {
        resizeTimer = null;
        sendResize();
      }, 100);
    };

    window.addEventListener("resize", onResize);

    const onDataDisposable = term.onData((data) => {
      if (ws.readyState !== WebSocket.OPEN) return;
      ws.send(encoder.encode(data));
    });

    ws.onopen = () => {
      setSessionStatus("connected");
      setErr(null);
      term.writeln("\x1b[33m[connected]\x1b[0m");
      sendJson({ type: "hello", client_name: "web", client_id: crypto.randomUUID() });
      sendJson({ type: "tail", bytes: 8192 });
      sendResize();
    };

    ws.onmessage = (ev) => {
      if (typeof ev.data === "string") {
        try {
          const msg = JSON.parse(ev.data) as TerminalControlMessage;
          switch (msg.type) {
            case "status":
              if (typeof msg.backend === "string") setBackend(msg.backend);
              if (typeof msg.clients_attached === "number") setClients(msg.clients_attached);
              break;
            case "error":
              term.writeln(`\r\n\x1b[31m[error]\x1b[0m ${msg.message || "unknown error"}`);
              break;
            case "warning":
              term.writeln(`\r\n\x1b[33m[warning]\x1b[0m ${msg.message || "warning"}`);
              break;
            case "tail_begin":
            case "hello":
            case "pong":
              break;
            default:
              term.writeln(`\r\n[ws] ${ev.data}`);
          }
        } catch {
          term.writeln(`\r\n[ws] ${ev.data}`);
        }
        return;
      }

      const bytes = new Uint8Array(ev.data as ArrayBuffer);
      const text = decoder.decode(bytes, { stream: true });
      if (text) {
        term.write(text);
      }
    };

    ws.onerror = () => {
      setErr("WebSocket error");
    };

    ws.onclose = () => {
      setSessionStatus("disconnected");
      const remaining = decoder.decode();
      if (remaining) term.write(remaining);
      term.writeln("\r\n\x1b[33m[disconnected]\x1b[0m");
    };

    return () => {
      window.removeEventListener("resize", onResize);
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      try {
        onDataDisposable.dispose();
      } catch {}
      try {
        ws.close();
      } catch {}
      try {
        term.dispose();
      } catch {}
    };
  }, [id, fit, sessionNonce]);

  return (
    <section>
      <div className="link-row">
        <Link className="btn btn-link" to="/">
          Back to Dashboard
        </Link>
      </div>

      <header className="page-header">
        <h1 className="page-title">Terminal</h1>
        <p className="page-subtitle mono-text">{id}</p>
      </header>

      <div className="surface-card card-content">
        <div className="terminal-toolbar">
          <div className="health-row">
            <span className={statusBadgeClass(sessionStatus)}>{sessionStatus}</span>
            <span className="health-pill">
              backend: <strong className="mono-text">{backend}</strong>
            </span>
            <span className="health-pill">
              attached clients: <strong>{clients}</strong>
            </span>
          </div>

          <button
            className="btn btn-secondary"
            type="button"
            onClick={() => setSessionNonce((v) => v + 1)}
            disabled={sessionStatus === "connecting"}
          >
            Reconnect
          </button>
        </div>

        {err && (
          <div className="alert error" role="status" aria-live="polite">
            {err}
          </div>
        )}
        <div ref={ref} className="terminal-frame space-top" />
      </div>
    </section>
  );
}
