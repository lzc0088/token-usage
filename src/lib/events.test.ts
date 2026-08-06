import { describe, it, expect } from "vitest";
import {
  QUOTA_UPDATED,
  CONFIG_CHANGED,
  TODAY_UPDATED,
  RATE_UPDATED,
  TRAY_REFRESH,
  COLLECTION_UPDATED,
  COLLECTION_ERROR,
  COPILOT_LOGIN_STATUS,
  CODEX_LOGIN_STATUS,
} from "./events";

describe("event constants", () => {
  it("QUOTA_UPDATED is 'quota:updated'", () => {
    expect(QUOTA_UPDATED).toBe("quota:updated");
  });

  it("CONFIG_CHANGED is 'config:changed'", () => {
    expect(CONFIG_CHANGED).toBe("config:changed");
  });

  it("TODAY_UPDATED is 'today:updated'", () => {
    expect(TODAY_UPDATED).toBe("today:updated");
  });

  it("RATE_UPDATED is 'rate:updated'", () => {
    expect(RATE_UPDATED).toBe("rate:updated");
  });

  it("TRAY_REFRESH is 'tray:refresh'", () => {
    expect(TRAY_REFRESH).toBe("tray:refresh");
  });

  it("COLLECTION_UPDATED is 'collection:updated'", () => {
    expect(COLLECTION_UPDATED).toBe("collection:updated");
  });

  it("COLLECTION_ERROR is 'collection:error'", () => {
    expect(COLLECTION_ERROR).toBe("collection:error");
  });

  it("COPILOT_LOGIN_STATUS is 'copilot:login_status'", () => {
    expect(COPILOT_LOGIN_STATUS).toBe("copilot:login_status");
  });

  it("CODEX_LOGIN_STATUS is 'codex:login_status'", () => {
    expect(CODEX_LOGIN_STATUS).toBe("codex:login_status");
  });

  it("all constants are non-empty strings", () => {
    const values = [
      QUOTA_UPDATED, CONFIG_CHANGED, TODAY_UPDATED, RATE_UPDATED,
      TRAY_REFRESH, COLLECTION_UPDATED, COLLECTION_ERROR,
      COPILOT_LOGIN_STATUS, CODEX_LOGIN_STATUS,
    ];
    for (const v of values) {
      expect(v.length).toBeGreaterThan(0);
    }
  });

  it("all constants are unique", () => {
    const values = [
      QUOTA_UPDATED, CONFIG_CHANGED, TODAY_UPDATED, RATE_UPDATED,
      TRAY_REFRESH, COLLECTION_UPDATED, COLLECTION_ERROR,
      COPILOT_LOGIN_STATUS, CODEX_LOGIN_STATUS,
    ];
    const unique = new Set(values);
    expect(unique.size).toBe(values.length);
  });
});
