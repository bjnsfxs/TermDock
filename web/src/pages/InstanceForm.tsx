import React, { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { createInstance, getInstance, updateInstance } from "../lib/api";
import type { ConfigMode, RestartPolicy } from "../lib/types";
import { buildInstancePayload, parseArgsInput, type InstanceFormState } from "./instance-form-utils";

type Props = { mode: "create" | "edit" };

const defaultForm: InstanceFormState = {
  name: "",
  command: "",
  args: [],
  cwd: "",
  use_pty: true,
  config_mode: "none",
  config_path: "",
  config_filename: "config.yaml",
  config_content: "",
  restart_policy: "never",
  auto_start: false,
  enabled: true,
};

export default function InstanceForm({ mode }: Props) {
  const nav = useNavigate();
  const params = useParams();
  const [form, setForm] = useState(defaultForm);
  const [envText, setEnvText] = useState("{}");
  const [err, setErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [loadingExisting, setLoadingExisting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (mode !== "edit" || !params.id) return;
      setLoadingExisting(true);
      setErr(null);
      try {
        const res = await getInstance(params.id);
        if (cancelled) return;
        setForm({
          ...defaultForm,
          ...res.instance,
          args: res.instance?.args || [],
          cwd: res.instance?.cwd || "",
          config_path: res.instance?.config_path || "",
          config_filename: res.instance?.config_filename || "config.yaml",
          config_content: res.instance?.config_content || "",
        });
        setEnvText(JSON.stringify(res.instance?.env || {}, null, 2));
      } catch (e) {
        if (!cancelled) setErr((e as Error).message);
      } finally {
        if (!cancelled) setLoadingExisting(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [mode, params.id]);

  function set<K extends keyof typeof form>(k: K, v: (typeof form)[K]) {
    setForm((p) => ({ ...p, [k]: v }));
  }

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setErr(null);
    setSaving(true);

    try {
      const payload = buildInstancePayload(form, envText);
      if (mode === "create") {
        await createInstance(payload);
      } else {
        await updateInstance(params.id!, payload);
      }
      nav("/");
    } catch (e) {
      setErr((e as Error).message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="form-shell">
      <div className="link-row">
        <Link className="btn btn-link" to="/">
          Back to Dashboard
        </Link>
      </div>

      <header className="page-header">
        <h1 className="page-title">{mode === "create" ? "Create New CLI Instance" : "Edit CLI Instance"}</h1>
        <p className="page-subtitle">Configure command, runtime options, and startup policy.</p>
      </header>

      {err && <div className="alert error">{err}</div>}
      {loadingExisting && <div className="alert info">Loading existing instance...</div>}

      <form className="surface-card form-card" onSubmit={onSubmit}>
        <div className="card-content">
          <label className="field">
            <span className="field-label">Enabled</span>
            <select
              className="select-input"
              value={form.enabled ? "true" : "false"}
              onChange={(e) => set("enabled", e.target.value === "true")}
              disabled={saving}
            >
              <option value="true">Enabled</option>
              <option value="false">Disabled</option>
            </select>
          </label>

          <label className="field">
            <span className="field-label">Instance Name</span>
            <input
              className="text-input"
              value={form.name}
              onChange={(e) => set("name", e.target.value)}
              placeholder="my-awesome-agent"
              autoFocus
              disabled={saving}
            />
          </label>

          <label className="field">
            <span className="field-label">Command</span>
            <input
              className="text-input mono-text"
              value={form.command}
              onChange={(e) => set("command", e.target.value)}
              placeholder="npm run start"
              disabled={saving}
            />
          </label>

          <label className="field">
            <span className="field-label">Arguments (space separated)</span>
            <textarea
              className="text-area mono-text"
              value={form.args.join(" ")}
              onChange={(e) => set("args", parseArgsInput(e.target.value))}
              rows={3}
              placeholder="--port 3000 --verbose"
              disabled={saving}
            />
          </label>

          <label className="field">
            <span className="field-label">Working Directory (optional)</span>
            <input
              className="text-input mono-text"
              value={form.cwd}
              onChange={(e) => set("cwd", e.target.value)}
              placeholder="/usr/src/app"
              disabled={saving}
            />
          </label>

          <div className="grid-2">
            <label className="field">
              <span className="field-label">Config Mode</span>
              <select
                className="select-input"
                value={form.config_mode}
                onChange={(e) => set("config_mode", e.target.value as ConfigMode)}
                disabled={saving}
              >
                <option value="none">None</option>
                <option value="path">Path</option>
                <option value="inline">Inline</option>
              </select>
            </label>

            <label className="field">
              <span className="field-label">Restart Policy</span>
              <select
                className="select-input"
                value={form.restart_policy}
                onChange={(e) => set("restart_policy", e.target.value as RestartPolicy)}
                disabled={saving}
              >
                <option value="never">Never</option>
                <option value="on-failure">On Failure</option>
                <option value="always">Always</option>
              </select>
            </label>
          </div>

          {form.config_mode === "path" && (
            <label className="field">
              <span className="field-label">Config Path</span>
              <input
                className="text-input mono-text"
                value={form.config_path}
                onChange={(e) => set("config_path", e.target.value)}
                placeholder="C:/path/to/config.yaml"
                disabled={saving}
              />
            </label>
          )}

          {form.config_mode === "inline" && (
            <>
              <label className="field">
                <span className="field-label">Config Filename</span>
                <input
                  className="text-input mono-text"
                  value={form.config_filename}
                  onChange={(e) => set("config_filename", e.target.value)}
                  placeholder="config.yaml"
                  disabled={saving}
                />
              </label>
              <label className="field">
                <span className="field-label">Config Content</span>
                <textarea
                  className="text-area mono-text"
                  value={form.config_content}
                  onChange={(e) => set("config_content", e.target.value)}
                  rows={8}
                  disabled={saving}
                />
              </label>
            </>
          )}

          <label className="field">
            <span className="field-label">Environment Variables (JSON object)</span>
            <textarea
              className="text-area mono-text"
              value={envText}
              onChange={(e) => setEnvText(e.target.value)}
              rows={6}
              placeholder={'{\n  "NODE_ENV": "production"\n}'}
              disabled={saving}
            />
          </label>

          <div className="checkbox-grid">
            <label className="checkbox-item">
              <input type="checkbox" checked={form.use_pty} onChange={(e) => set("use_pty", e.target.checked)} disabled={saving} />
              <span>
                <div className="checkbox-title">Use PTY</div>
                <div className="checkbox-help">Allocate pseudo-terminal for interactive processes.</div>
              </span>
            </label>

            <label className="checkbox-item">
              <input type="checkbox" checked={form.auto_start} onChange={(e) => set("auto_start", e.target.checked)} disabled={saving} />
              <span>
                <div className="checkbox-title">Auto Start</div>
                <div className="checkbox-help">Start instance immediately after saving configuration.</div>
              </span>
            </label>
          </div>
        </div>

        <div className="form-footer">
          <button className="btn btn-secondary" type="button" onClick={() => nav(-1)} disabled={saving}>
            Cancel
          </button>
          <button className="btn btn-primary" type="submit" disabled={saving}>
            {saving ? "Saving..." : mode === "create" ? "Create Instance" : "Save Changes"}
          </button>
        </div>
      </form>
    </section>
  );
}
