import { describe, expect, it, afterEach } from "vitest";
import { applyTheme } from "./theme";

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("applyTheme", () => {
  it("sets data-theme for explicit modes", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("removes data-theme for system mode", () => {
    applyTheme("dark");
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
