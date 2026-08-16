import { describe, expect, it } from "vitest";
import {
  LogEntry,
  deriveRunActions,
  filterOutputsByRun,
  mergeLogs,
  OutputPayload,
} from "./projectRuntime";

const out = (seq: number, runId = "r1"): OutputPayload => ({
  runId,
  projectId: "p1",
  seq,
  stream: "stdout",
  text: `line-${seq}`,
  truncated: false,
});

describe("mergeLogs", () => {
  it("deduplicates by seq and sorts ascending", () => {
    const a: LogEntry[] = [
      { seq: 1, stream: "stdout", text: "a", truncated: false },
      { seq: 3, stream: "stderr", text: "c", truncated: false },
    ];
    const b: LogEntry[] = [
      { seq: 3, stream: "stderr", text: "c-updated", truncated: false },
      { seq: 2, stream: "stdout", text: "b", truncated: false },
    ];
    const merged = mergeLogs(a, b);
    expect(merged.map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(merged[2].text).toBe("c-updated");
  });

  it("handles empty inputs", () => {
    expect(mergeLogs([], [])).toEqual([]);
    expect(mergeLogs([], [{ seq: 1, stream: "stdout", text: "x", truncated: false }])).toHaveLength(1);
  });
});

describe("filterOutputsByRun", () => {
  it("keeps only matching runId", () => {
    const payloads = [out(1, "r1"), out(2, "r2"), out(3, "r1")];
    const filtered = filterOutputsByRun(payloads, "r1");
    expect(filtered.map((p) => p.seq)).toEqual([1, 3]);
  });
});

describe("deriveRunActions", () => {
  it("no run allows start only", () => {
    expect(deriveRunActions(null)).toEqual({
      canStart: true,
      canStop: false,
      canRestart: false,
    });
  });

  it("running disables start, enables stop/restart", () => {
    expect(
      deriveRunActions({
        runId: "r1",
        projectId: "p1",
        state: "running",
        cwd: "C:\\proj",
        pid: 42,
        startedAt: 1,
        exitCode: null,
        errorCode: null,
        errorMessage: null,
        stopReason: null,
        summary: { executable: "node", args: [], cwd: "", env: {}, expected_port: null, preview_scheme: "http" },
      }),
    ).toEqual({ canStart: false, canStop: true, canRestart: true });
  });

  it("stopping disables everything", () => {
    const snap = {
      runId: "r1",
      projectId: "p1",
      state: "stopping" as const,
      cwd: "",
      pid: 1,
      startedAt: 1,
      exitCode: null,
      errorCode: null,
      errorMessage: null,
      stopReason: null,
      summary: { executable: "x", args: [], cwd: "", env: {}, expected_port: null, preview_scheme: "http" },
    };
    expect(deriveRunActions(snap)).toEqual({
      canStart: false,
      canStop: false,
      canRestart: false,
    });
  });

  it("failed allows start and restart", () => {
    const snap = {
      runId: "r1",
      projectId: "p1",
      state: "failed" as const,
      cwd: "",
      pid: null,
      startedAt: 1,
      exitCode: null,
      errorCode: "process_spawn_failed",
      errorMessage: "boom",
      stopReason: null,
      summary: { executable: "x", args: [], cwd: "", env: {}, expected_port: null, preview_scheme: "http" },
    };
    expect(deriveRunActions(snap)).toEqual({
      canStart: true,
      canStop: false,
      canRestart: true,
    });
  });
});
