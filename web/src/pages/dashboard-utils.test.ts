import { describe, expect, it } from "vitest";
import { canRestart, canStart, canStop, formatCpu, patchInstanceRuntime } from "./dashboard-utils";
import type { Instance } from "../lib/types";

describe("dashboard-utils", () => {
  it("formats cpu percentage with clamp", () => {
    expect(formatCpu(9.12)).toEqual({ label: "9.1%", percent: 9.12 });
    expect(formatCpu(120)).toEqual({ label: "100%", percent: 100 });
    expect(formatCpu(null)).toEqual({ label: "-", percent: 0 });
  });

  it("patches runtime for matched instance only", () => {
    const instances: Instance[] = [
      {
        id: "a",
        name: "one",
        enabled: true,
        command: "cmd",
        args: [],
        cwd: null,
        env: {},
        use_pty: true,
        config_mode: "none",
        config_path: null,
        config_filename: null,
        config_content: null,
        restart_policy: "never",
        auto_start: false,
        runtime: { status: "running", pid: 10, clients_attached: 1 },
      },
      {
        id: "b",
        name: "two",
        enabled: true,
        command: "cmd",
        args: [],
        cwd: null,
        env: {},
        use_pty: true,
        config_mode: "none",
        config_path: null,
        config_filename: null,
        config_content: null,
        restart_policy: "never",
        auto_start: false,
        runtime: { status: "stopped", pid: null, clients_attached: 0 },
      },
    ];

    const result = patchInstanceRuntime(instances, "a", { cpu_percent: 10, mem_bytes: 20 });
    expect(result.found).toBe(true);
    expect(result.instances[0].runtime).toMatchObject({
      status: "running",
      pid: 10,
      clients_attached: 1,
      cpu_percent: 10,
      mem_bytes: 20,
    });
    expect(result.instances[1]).toBe(instances[1]);
  });

  it("maps action availability by runtime status", () => {
    expect(canStart("running")).toBe(false);
    expect(canStart("stopped")).toBe(true);
    expect(canStop("running")).toBe(true);
    expect(canStop("stopped")).toBe(false);
    expect(canStop("stopping")).toBe(false);
    expect(canRestart("running")).toBe(true);
    expect(canRestart("error")).toBe(false);
  });
});
