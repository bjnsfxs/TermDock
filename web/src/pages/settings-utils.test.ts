import { describe, expect, it } from "vitest";
import { validatePortInput } from "./settings-utils";

describe("validatePortInput", () => {
  it("accepts valid integer port", () => {
    expect(validatePortInput("8765")).toEqual({ ok: true, port: 8765 });
  });

  it("rejects invalid ports", () => {
    expect(validatePortInput("0")).toEqual({ ok: false, message: "port must be an integer in [1, 65535]." });
    expect(validatePortInput("70000")).toEqual({ ok: false, message: "port must be an integer in [1, 65535]." });
    expect(validatePortInput("1.2")).toEqual({ ok: false, message: "port must be an integer in [1, 65535]." });
    expect(validatePortInput("abc")).toEqual({ ok: false, message: "port must be an integer in [1, 65535]." });
  });
});
