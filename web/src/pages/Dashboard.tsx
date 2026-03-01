import React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import {
  buildWsUrl,
  deleteInstance,
  listInstances,
  restartInstance,
  startInstance,
  stopInstance,
} from "../lib/api";

type EventsConnection = "connecting" | "connected" | "disconnected";
type NoticeLevel = "info" | "error";

const POLL_INTERVAL_MS = 2000;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 10000;

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function formatCpu(value: unknown): { label: string; percent: number } {
  const cpu = asNumber(value);
  if (cpu === null) return { label: "-", percent: 0 };
  const clamped = Math.max(0, Math.min(100, cpu));
  const digits = clamped < 10 ? 1 : 0;
  return { label: `${clamped.toFixed(digits)}%`, percent: clamped };
}

function formatMemory(value: unknown): { label: string; percent: number } {
  const bytes = asNumber(value);
  if (bytes === null || bytes < 0) return { label: "-", percent: 0 };
  const mb = bytes / (1024 * 1024);
  const gb = mb / 1024;
  const label = gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(mb)} MB`;
  const percent = Math.max(0, Math.min(100, (mb / 2048) * 100));
  return { label, percent };
}

function statusClass(status: string | undefined): string {
  if (!status) return "status-badge status-generic";
  switch (status) {
    case "running":
      return "status-badge status-running";
    case "stopped":
      return "status-badge status-stopped";
    case "error":
      return "status-badge status-error";
    default:
      return "status-badge status-generic";
  }
}

export default function Dashboard() {
  const qc = useQueryClient();
  const [pollingEnabled, setPollingEnabled] = React.useState(true);
  const [eventsConnection, setEventsConnection] = React.useState<EventsConnection>("connecting");
  const [notice, setNotice] = React.useState<{ level: NoticeLevel; text: string } | null>(null);
  const [filterText, setFilterText] = React.useState("");

  const q = useQuery({
    queryKey: ["instances"],
    queryFn: listInstances,
    refetchInterval: pollingEnabled ? POLL_INTERVAL_MS : false,
    refetchIntervalInBackground: pollingEnabled,
  });

  React.useEffect(() => {
    let disposed = false;
    let ws: WebSocket | null = null;
    let reconnectTimer: number | null = null;
    let reconnectAttempt = 0;

    const scheduleReconnect = () => {
      if (disposed) return;
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
      const delay = Math.min(RECONNECT_BASE_MS * 2 ** reconnectAttempt, RECONNECT_MAX_MS);
      reconnectAttempt += 1;
      reconnectTimer = window.setTimeout(connect, delay);
    };

    const connect = () => {
      if (disposed) return;
      setEventsConnection("connecting");

      try {
        ws = new WebSocket(buildWsUrl("/ws/v1/events"));
      } catch (err) {
        setEventsConnection("disconnected");
        setPollingEnabled(true);
        setNotice({ level: "error", text: (err as Error).message });
        scheduleReconnect();
        return;
      }

      ws.onopen = () => {
        if (disposed) return;
        reconnectAttempt = 0;
        setEventsConnection("connected");
        setPollingEnabled(false);
      };

      ws.onmessage = (event) => {
        if (typeof event.data !== "string") return;

        let msg: any;
        try {
          msg = JSON.parse(event.data);
        } catch {
          return;
        }

        if (msg?.type === "instance_status" && typeof msg.id === "string" && msg.runtime) {
          let shouldRefetch = false;
          qc.setQueryData(["instances"], (previous: any) => {
            if (!previous || !Array.isArray(previous.instances)) {
              shouldRefetch = true;
              return previous;
            }

            let found = false;
            const instances = previous.instances.map((it: any) => {
              if (it.id !== msg.id) return it;
              found = true;
              return { ...it, runtime: msg.runtime };
            });

            if (!found) {
              shouldRefetch = true;
              return previous;
            }

            return { ...previous, instances };
          });

          if (shouldRefetch) {
            void qc.invalidateQueries({ queryKey: ["instances"] });
          }
          return;
        }

        if (msg?.type === "notice" && typeof msg.message === "string") {
          setNotice({ level: "info", text: msg.message });
          if (msg.level === "warn") {
            void qc.invalidateQueries({ queryKey: ["instances"] });
          }
        }
      };

      ws.onerror = () => {
        if (disposed) return;
        setEventsConnection("disconnected");
        setPollingEnabled(true);
      };

      ws.onclose = () => {
        if (disposed) return;
        setEventsConnection("disconnected");
        setPollingEnabled(true);
        scheduleReconnect();
      };
    };

    connect();

    return () => {
      disposed = true;
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
      if (ws) {
        try {
          ws.close();
        } catch {}
      }
    };
  }, [qc]);

  const instances: any[] = React.useMemo(() => {
    const all = q.data?.instances || [];
    const keyword = filterText.trim().toLowerCase();
    if (!keyword) return all;
    return all.filter((it: any) => {
      return [it.name, it.command, it.cwd]
        .filter((v) => typeof v === "string" && v.length > 0)
        .some((v) => (v as string).toLowerCase().includes(keyword));
    });
  }, [q.data?.instances, filterText]);

  async function runAction(fn: () => Promise<unknown>) {
    setNotice(null);
    try {
      await fn();
      await qc.invalidateQueries({ queryKey: ["instances"] });
    } catch (err) {
      setNotice({ level: "error", text: (err as Error).message });
    }
  }

  return (
    <section>
      <header className="page-header">
        <h1 className="page-title">Dashboard</h1>
        <p className="page-subtitle">Monitor all CLI instances and control runtime actions.</p>
      </header>

      <div className="surface-card card-content dashboard-topbar">
        <div className="health-row">
          <div className="health-pill">
            <span className={`dot ${eventsConnection === "connected" ? "good" : eventsConnection === "connecting" ? "warn" : "bad"}`} />
            events: <strong>{eventsConnection}</strong>
          </div>
          <div className="health-pill">
            <span className={`dot ${pollingEnabled ? "warn" : "good"}`} />
            fallback polling: <strong>{pollingEnabled ? `on (${POLL_INTERVAL_MS}ms)` : "off"}</strong>
          </div>
        </div>
        <div className="btn-row">
          <input
            className="text-input"
            type="search"
            placeholder="Filter instances"
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            style={{ width: 220 }}
          />
          <button className="btn btn-secondary" type="button" onClick={() => void qc.invalidateQueries({ queryKey: ["instances"] })}>
            Refresh All
          </button>
        </div>
      </div>

      {q.isLoading && <div className="alert info">Loading instances...</div>}
      {q.isError && <div className="alert error">{(q.error as Error).message}</div>}
      {notice && <div className={`alert ${notice.level === "error" ? "error" : "info"} space-top`}>{notice.text}</div>}

      <div className="instance-grid space-top">
        {instances.map((it) => {
          const runtime = it.runtime || {};
          const status = typeof runtime.status === "string" ? runtime.status : "unknown";
          const cpu = formatCpu(runtime.cpu_percent);
          const mem = formatMemory(runtime.mem_bytes);
          return (
            <article key={it.id} className="instance-card">
              <div className="instance-head">
                <div>
                  <h2 className="instance-name">{it.name}</h2>
                  <p className="instance-cmd mono-text">{it.command}</p>
                </div>
                <span className={statusClass(status)}>{status}</span>
              </div>

              <div className="metric-grid">
                <div className="metric-row">
                  <span className="metric-key">cwd</span>
                  <span className="metric-value mono-text" title={it.cwd || "-"}>
                    {it.cwd || "-"}
                  </span>
                </div>
                <div className="metric-row">
                  <span className="metric-key">pid</span>
                  <span className="metric-value mono-text">{runtime.pid ?? "-"}</span>
                </div>
                <div className="metric-row">
                  <span className="metric-key">cpu</span>
                  <div className="metric-with-bar">
                    <div className="bar-track">
                      <div className="bar-fill" style={{ width: `${cpu.percent}%` }} />
                    </div>
                    <span className="metric-value mono-text">{cpu.label}</span>
                  </div>
                </div>
                <div className="metric-row">
                  <span className="metric-key">mem</span>
                  <div className="metric-with-bar">
                    <div className="bar-track">
                      <div className="bar-fill" style={{ width: `${mem.percent}%` }} />
                    </div>
                    <span className="metric-value mono-text">{mem.label}</span>
                  </div>
                </div>
              </div>

              <hr className="card-divider" />

              <div className="card-actions">
                <button className="btn btn-success" onClick={() => void runAction(() => startInstance(it.id))}>
                  Start
                </button>
                <button className="btn btn-danger" onClick={() => void runAction(() => stopInstance(it.id))}>
                  Stop
                </button>
                <button className="btn btn-warning" onClick={() => void runAction(() => restartInstance(it.id))}>
                  Restart
                </button>
                <Link className="btn btn-secondary" to={`/instances/${it.id}/term`}>
                  Terminal
                </Link>
                <span className="push-right" />
                <Link className="btn btn-secondary" to={`/instances/${it.id}/edit`}>
                  Edit
                </Link>
                <button
                  className="btn btn-secondary"
                  onClick={() =>
                    void runAction(async () => {
                      if (!window.confirm("Delete instance?")) return;
                      await deleteInstance(it.id);
                    })
                  }
                >
                  Delete
                </button>
              </div>
            </article>
          );
        })}
      </div>

      {instances.length === 0 && !q.isLoading && (
        <div className="alert info space-top">
          No instances match current results. Create one from <Link to="/instances/new">New Instance</Link>.
        </div>
      )}
    </section>
  );
}
