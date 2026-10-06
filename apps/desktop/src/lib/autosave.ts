/**
 * Saves the latest text after a pause in typing, one save at a time.
 *
 * The one-at-a-time part is the point: the first save of a new note creates
 * its file and hands back the path, so a second save sent before that returns
 * would create a second file. While one is in flight, newer text only
 * replaces what is pending and goes out when it lands.
 */
export function createAutosaver(save: (text: string) => Promise<void>, delay = 600) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: string | null = null;
  let inflight: Promise<void> | null = null;

  const run = () => {
    if (inflight) return inflight;
    inflight = (async () => {
      try {
        while (pending !== null) {
          const text = pending;
          pending = null;
          try {
            await save(text);
          } catch {
            // A failed save is retried by the next keystroke; stopping here
            // would strand the loop with text still pending.
          }
        }
      } finally {
        // No await between the loop's last check and this: a `flush` cannot
        // slip in between and be told a save is still coming.
        inflight = null;
      }
    })();
    return inflight;
  };

  return {
    /** The text as it is now; saved once typing pauses. */
    schedule(text: string) {
      pending = text;
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    },
    /** Saves now, for a note being left. Resolves when nothing is left to save. */
    flush(): Promise<void> {
      clearTimeout(timer);
      return pending !== null || inflight ? run() : Promise.resolve();
    },
  };
}
