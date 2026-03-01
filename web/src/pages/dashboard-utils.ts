import type { Instance, RuntimeSnapshot, RuntimeStatus } from "../lib/types";

export function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

export function formatCpu(value: unknown): { label: string; percent: number } {
  const cpu = asNumber(value);
  if (cpu === null) return { label: "-", percent: 0 };
  const clamped = Math.max(0, Math.min(100, cpu));
  const digits = clamped < 10 ? 1 : 0;
  return { label: `${clamped.toFixed(digits)}%`, percent: clamped };
}

export function formatMemory(value: unknown): { label: string; percent: number } {
  const bytes = asNumber(value);
  if (bytes === null || bytes < 0) return { label: "-", percent: 0 };
  const mb = bytes / (1024 * 1024);
  const gb = mb / 1024;
  const label = gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(mb)} MB`;
  const percent = Math.max(0, Math.min(100, (mb / 2048) * 100));
  return { label, percent };
}

export function statusClass(status: RuntimeStatus | undefined): string {
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

export function canStart(status: RuntimeStatus | undefined): boolean {
  return status !== "running" && status !== "starting" && status !== "stopping";
}

export function canStop(status: RuntimeStatus | undefined): boolean {
  return status === "running" || status === "starting";
}

export function canRestart(status: RuntimeStatus | undefined): boolean {
  return status === "running";
}

export function canOpenTerminal(status: RuntimeStatus | undefined): boolean {
  return status === "running";
}

export function patchInstanceRuntime(
  instances: Instance[],
  id: string,
  runtime: Partial<RuntimeSnapshot>,
): { instances: Instance[]; found: boolean } {
  let found = false;
  const patched = instances.map((instance) => {
    if (instance.id !== id) return instance;
    found = true;
    return { ...instance, runtime: { ...(instance.runtime ?? {}), ...runtime } };
  });
  return { instances: patched, found };
}
