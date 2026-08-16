import { describe, expect, it } from "vitest";
import {
  commandLineOf,
  expectedPortOf,
  formatRunTime,
  runStateLabel,
} from "./runCenter";
import type { RunSnapshot } from "../../projects/lib/projectRuntime";

const snap = (over: Partial<RunSnapshot> = {}): RunSnapshot => ({
  runId: "r1",
  projectId: "p1",
  state: "running",
  cwd: "C:\\proj",
  pid: 123,
  startedAt: 1700000000,
  exitCode: null,
  errorCode: null,
  errorMessage: null,
  stopReason: null,
  summary: {
    executable: "npm",
    args: ["run", "dev"],
    cwd: "C:\\proj",
    env: {},
    expected_port: 3000,
    preview_scheme: "http",
  },
  ...over,
});

describe("runStateLabel", () => {
  it("maps all states", () => {
    expect(runStateLabel("starting")).toBe("启动中");
    expect(runStateLabel("running")).toBe("运行中");
    expect(runStateLabel("stopping")).toBe("停止中");
    expect(runStateLabel("exited")).toBe("已退出");
    expect(runStateLabel("failed")).toBe("失败");
    expect(runStateLabel("weird")).toBe("weird");
  });
});

describe("expectedPortOf", () => {
  it("reads port from summary", () => {
    expect(expectedPortOf(snap())).toBe(3000);
    expect(expectedPortOf(snap({ summary: { ...snap().summary, expected_port: null } }))).toBeNull();
  });
});

describe("commandLineOf", () => {
  it("joins executable and args", () => {
    expect(commandLineOf(snap())).toBe("npm run dev");
    expect(commandLineOf(snap({ summary: { ...snap().summary, args: [] } }))).toBe("npm");
  });
});

describe("formatRunTime", () => {
  it("formats timestamp or dash", () => {
    // 1700000000 = 2023-11-14 22:13:20 UTC；本地时区可能不同，只校验格式长度。
    expect(formatRunTime(1700000000)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    expect(formatRunTime(null)).toBe("—");
    expect(formatRunTime(undefined)).toBe("—");
  });
});
