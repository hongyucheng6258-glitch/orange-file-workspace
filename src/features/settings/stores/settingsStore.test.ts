import { describe, expect, it } from "vitest";
import { applyPatch, type AppSettings } from "./settingsStore";

const base: AppSettings = {
  general: {
    launch_behavior: "home",
    default_import_mode: "managed",
    duplicate_policy: "skip",
    preview_size_limit_mb: 256,
    minimize_to_tray: true,
  },
  appearance: { theme_mode: "system", density: "comfortable" },
  terminal: { font_size: 14, cursor_style: "bar", theme: "campbell" },
  ignore: { custom_rules: [] },
  backup: {
    enabled: false,
    frequency: "daily",
    run_time: "02:00",
    retention_count: 7,
    backup_type: "full",
  },
  storage: { data_dir: "", managed_dir: "" },
};

describe("applyPatch", () => {
  it("patches nested field without mutating input", () => {
    const next = applyPatch(base, "appearance.theme_mode", "dark");
    expect(next.appearance.theme_mode).toBe("dark");
    expect(base.appearance.theme_mode).toBe("system");
    expect(next).not.toBe(base);
    expect(next.appearance).not.toBe(base.appearance);
  });

  it("patches number fields", () => {
    const next = applyPatch(base, "general.preview_size_limit_mb", 512);
    expect(next.general.preview_size_limit_mb).toBe(512);
  });
});
