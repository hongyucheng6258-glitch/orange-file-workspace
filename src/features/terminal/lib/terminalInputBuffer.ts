export type TerminalInputWriter = (sessionId: number, data: string) => Promise<void>;

export interface TerminalInputBuffer {
  push: (data: string) => void;
  attach: (sessionId: number) => Promise<void>;
  reset: () => void;
}

/** Keeps keystrokes ordered and preserves input typed while a PTY is starting. */
export function createTerminalInputBuffer(write: TerminalInputWriter): TerminalInputBuffer {
  let sessionId: number | null = null;
  let queued: string[] = [];
  let writes = Promise.resolve();
  let generation = 0;

  const enqueue = (id: number, data: string, currentGeneration: number) => {
    writes = writes
      .then(() => {
        if (generation !== currentGeneration || sessionId !== id) return;
        return write(id, data);
      })
      .catch(() => undefined);
  };

  return {
    push(data) {
      if (sessionId == null) {
        queued.push(data);
        return;
      }
      enqueue(sessionId, data, generation);
    },

    async attach(id) {
      sessionId = id;
      const currentGeneration = generation;
      const pending = queued;
      queued = [];
      pending.forEach((data) => enqueue(id, data, currentGeneration));
      await writes;
    },

    reset() {
      generation += 1;
      sessionId = null;
      queued = [];
      writes = Promise.resolve();
    },
  };
}
