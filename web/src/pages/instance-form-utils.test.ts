import { describe, expect, it } from "vitest";
import { buildInstancePayload, parseEnvJson, type InstanceFormState } from "./instance-form-utils";

const baseForm: InstanceFormState = {
  name: "demo",
  enabled: true,
  command: "node",
  args: ["app.js"],
  cwd: "C:/app",
  use_pty: true,
  config_mode: "none",
  config_path: "",
  config_filename: "config.yaml",
  config_content: "abc",
  restart_policy: "never",
  auto_start: false,
};

describe("instance-form-utils", () => {
  it("parses env object and stringifies non-string values", () => {
    expect(parseEnvJson('{"A":"1","B":2}')).toEqual({ A: "1", B: "2" });
  });

  it("throws on invalid env json", () => {
    expect(() => parseEnvJson("{invalid")).toThrow("Environment variables must be valid JSON.");
    expect(() => parseEnvJson("[]")).toThrow("Environment variables must be a JSON object.");
  });

  it("builds payload for inline config mode", () => {
    const payload = buildInstancePayload({ ...baseForm, config_mode: "inline" }, '{"X":"Y"}');
    expect(payload).toMatchObject({
      config_mode: "inline",
      config_filename: "config.yaml",
      config_content: "abc",
      config_path: null,
      env: { X: "Y" },
      cwd: "C:/app",
    });
  });

  it("validates required fields", () => {
    expect(() => buildInstancePayload({ ...baseForm, name: " " }, "{}")).toThrow("Instance name is required.");
    expect(() => buildInstancePayload({ ...baseForm, command: " " }, "{}")).toThrow("Command is required.");
  });
});
