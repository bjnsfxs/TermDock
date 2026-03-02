import { afterEach, describe, expect, it } from "vitest";
import { buildWsUrl, getActiveProfile, resolveDefaultBaseUrl, syncDesktopDaemonProfile } from "./api";

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

describe("resolveDefaultBaseUrl", () => {
  it("uses current origin for http/https protocols", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "https:",
        origin: "https://daemon.example.com",
      })
    ).toBe("https://daemon.example.com");
  });

  it("keeps localhost origin for local web development", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "http:",
        origin: "http://localhost:5173",
      })
    ).toBe("http://localhost:5173");
  });

  it("falls back to loopback for tauri localhost asset origin", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "http:",
        origin: "http://tauri.localhost",
      })
    ).toBe("http://127.0.0.1:8765");
  });

  it("falls back to loopback for https custom-scheme localhost asset origin", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "https:",
        origin: "https://myapp.localhost",
      })
    ).toBe("http://127.0.0.1:8765");
  });

  it("falls back to loopback when origin is malformed", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "https:",
        origin: "not a url",
      })
    ).toBe("http://127.0.0.1:8765");
  });

  it("falls back to loopback for non-http protocols", () => {
    expect(
      resolveDefaultBaseUrl({
        protocol: "tauri:",
        origin: "tauri://localhost",
      })
    ).toBe("http://127.0.0.1:8765");
  });
});

describe("syncDesktopDaemonProfile", () => {
  it("creates desktop local profile and makes it active", () => {
    syncDesktopDaemonProfile({
      baseUrl: "http://127.0.0.1:8765",
      authToken: "master-token",
    });

    const active = getActiveProfile();
    expect(active.id).toBe("desktop-local");
    expect(active.baseUrl).toBe("http://127.0.0.1:8765");
    expect(active.token).toBe("master-token");
  });

  it("keeps existing token when desktop status has no token", () => {
    syncDesktopDaemonProfile({
      baseUrl: "http://127.0.0.1:8765",
      authToken: "existing-token",
    });

    syncDesktopDaemonProfile({
      baseUrl: "http://127.0.0.1:8765",
      authToken: null,
    });

    const active = getActiveProfile();
    expect(active.id).toBe("desktop-local");
    expect(active.token).toBe("existing-token");
  });
});
