import { afterEach, describe, expect, it } from "vitest";
import { buildWsUrl } from "./api";

afterEach(() => {
  localStorage.clear();
});

describe("buildWsUrl", () => {
  it("builds ws url and appends encoded token", () => {
    localStorage.setItem("daemonBaseUrl", "http://127.0.0.1:8765/");
    localStorage.setItem("daemonToken", "token value");
    expect(buildWsUrl("/ws/v1/events")).toBe("ws://127.0.0.1:8765/ws/v1/events?token=token%20value");
  });

  it("keeps existing query params when token is present", () => {
    localStorage.setItem("daemonBaseUrl", "https://example.com");
    localStorage.setItem("daemonToken", "abc");
    expect(buildWsUrl("/ws/v1/term/1?tail=1")).toBe("wss://example.com/ws/v1/term/1?tail=1&token=abc");
  });

  it("does not append query when token is empty", () => {
    localStorage.setItem("daemonBaseUrl", "http://127.0.0.1:8765");
    localStorage.setItem("daemonToken", "");
    expect(buildWsUrl("/ws/v1/events")).toBe("ws://127.0.0.1:8765/ws/v1/events");
  });
});
