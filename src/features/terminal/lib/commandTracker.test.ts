import { describe, expect, it, vi } from "vitest";
import { createCommandTracker } from "./commandTracker";

describe("createCommandTracker", () => {
  it("submits a command on Enter", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("git status");
    t.push("\r");
    expect(submit).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledWith("git status");
  });

  it("trims whitespace and ignores empty commands", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("   ");
    t.push("\r");
    expect(submit).not.toHaveBeenCalled();
  });

  it("handles backspace", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("dirr");
    t.push("\x7f");
    t.push("\r");
    expect(submit).toHaveBeenCalledWith("dir");
  });

  it("clears buffer on Ctrl+C", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("abc");
    t.push("\x03");
    t.push("\r");
    expect(submit).not.toHaveBeenCalled();
  });

  it("skips escape sequences (arrow keys)", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    // 模拟方向键上 + 输入，再回车。
    t.push("\x1b[A");
    t.push("ls");
    t.push("\r");
    expect(submit).toHaveBeenCalledWith("ls");
  });

  it("does not treat arrow keys as command characters", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("\x1b[A");
    t.push("\x1b[B");
    t.push("pwd");
    t.push("\r");
    expect(submit).toHaveBeenCalledWith("pwd");
  });

  it("resets buffer", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("stale");
    t.reset();
    t.push("\r");
    expect(submit).not.toHaveBeenCalled();
  });

  it("accumulates multiple pushes before enter", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("echo ");
    t.push("hello");
    t.push("\r");
    expect(submit).toHaveBeenCalledWith("echo hello");
  });

  it("ignores newline characters in the command", () => {
    const submit = vi.fn();
    const t = createCommandTracker(submit);
    t.push("echo a\nb");
    t.push("\r");
    expect(submit).toHaveBeenCalledWith("echo ab");
  });
});
