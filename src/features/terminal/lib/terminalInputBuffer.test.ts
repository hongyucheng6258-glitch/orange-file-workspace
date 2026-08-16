import { describe, expect, it, vi } from "vitest";
import { createTerminalInputBuffer } from "./terminalInputBuffer";

describe("createTerminalInputBuffer", () => {
  it("queues input until a terminal session is ready", async () => {
    const write = vi.fn(async () => undefined);
    const input = createTerminalInputBuffer(write);

    input.push("echo ready");
    input.push("\r");
    expect(write).not.toHaveBeenCalled();

    await input.attach(42);
    expect(write.mock.calls).toEqual([
      [42, "echo ready"],
      [42, "\r"],
    ]);
  });

  it("writes new input directly after attaching", async () => {
    const write = vi.fn(async () => undefined);
    const input = createTerminalInputBuffer(write);

    await input.attach(7);
    input.push("dir\r");
    await Promise.resolve();

    expect(write).toHaveBeenCalledWith(7, "dir\r");
  });

  it("drops queued input when reset", async () => {
    const write = vi.fn(async () => undefined);
    const input = createTerminalInputBuffer(write);

    input.push("stale");
    input.reset();
    await input.attach(9);

    expect(write).not.toHaveBeenCalled();
  });
});
