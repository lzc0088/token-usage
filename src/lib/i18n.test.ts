import { describe, it, expect, beforeEach } from "vitest";
import { t, setLang, getLang } from "./i18n.svelte";

describe("i18n", () => {
  beforeEach(() => {
    // Reset to Chinese before each test
    setLang("zh");
  });

  describe("t()", () => {
    it("returns Chinese text by default", () => {
      expect(t("hero.todayTokens")).toBe("今日 Token");
    });

    it("returns English when lang is 'en'", () => {
      setLang("en");
      expect(t("hero.todayTokens")).toBe("Today Tokens");
    });

    it("returns the key itself when translation is missing", () => {
      expect(t("nonexistent.key")).toBe("nonexistent.key");
    });

    it("falls back to Chinese for unknown lang", () => {
      setLang("fr");
      expect(t("hero.todayTokens")).toBe("今日 Token");
    });

    it("returns English for valid en keys", () => {
      setLang("en");
      expect(t("period.day")).toBe("Today");
      expect(t("seg.overview")).toBe("Overview");
      // settings.account en value is "Account" (short form)
      expect(t("settings.account")).toBe("Account");
    });

    it("returns Chinese for valid zh keys", () => {
      setLang("zh");
      expect(t("period.day")).toBe("今日");
      expect(t("seg.overview")).toBe("总览");
      expect(t("settings.account")).toBe("账号额度");
    });

    it("handles empty string key", () => {
      expect(t("")).toBe("");
    });

    it("returns keys with interpolation placeholders", () => {
      // Keys with {n} placeholder - t() returns the raw string with placeholders
      const result = t("collection.archivedCount");
      expect(result).toContain("{count}");
    });
  });

  describe("setLang / getLang", () => {
    it("setLang('zh') makes getLang return 'zh'", () => {
      setLang("zh");
      expect(getLang()).toBe("zh");
    });

    it("setLang('en') makes getLang return 'en'", () => {
      setLang("en");
      expect(getLang()).toBe("en");
    });

    it("setLang with non-en non-zh falls back to zh", () => {
      setLang("de");
      expect(getLang()).toBe("zh");
    });

    it("setLang with empty string falls back to zh", () => {
      setLang("");
      expect(getLang()).toBe("zh");
    });
  });

  describe("language switching affects all keys", () => {
    const keys = [
      ["hero.refresh", "刷新", "Refresh"],
      ["period.month", "本月", "Month"],
      ["collection.title", "采集追踪", "Collection"],
      ["account.save", "保存", "Save"],
      ["window.themeDark", "深色", "Dark"],
    ];

    for (const [key, zh, en] of keys) {
      it(`"${key}" switches correctly`, () => {
        setLang("zh");
        expect(t(key)).toBe(zh);
        setLang("en");
        expect(t(key)).toBe(en);
        setLang("zh");
        expect(t(key)).toBe(zh);
      });
    }
  });
});
