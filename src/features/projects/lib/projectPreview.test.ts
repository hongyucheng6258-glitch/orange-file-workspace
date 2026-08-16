import { describe, expect, it } from "vitest";
import {
  canOpenPreview,
  ownershipLabel,
  previewSourceLabel,
} from "./projectPreview";

describe("previewSourceLabel", () => {
  it("maps all sources to chinese labels", () => {
    expect(previewSourceLabel("config")).toBe("配置端口");
    expect(previewSourceLabel("args")).toBe("命令行参数");
    expect(previewSourceLabel("log")).toBe("日志地址");
  });
});

describe("ownershipLabel", () => {
  it("maps confirmed and unconfirmed", () => {
    expect(ownershipLabel("confirmed")).toBe("端口归属已确认");
    expect(ownershipLabel("unconfirmed")).toBe("端口归属未确认");
  });
});

describe("canOpenPreview", () => {
  it("only allows preview while running", () => {
    expect(canOpenPreview("running")).toBe(true);
    expect(canOpenPreview("starting")).toBe(false);
    expect(canOpenPreview("stopping")).toBe(false);
    expect(canOpenPreview("exited")).toBe(false);
    expect(canOpenPreview("failed")).toBe(false);
    expect(canOpenPreview(null)).toBe(false);
    expect(canOpenPreview(undefined)).toBe(false);
  });
});
