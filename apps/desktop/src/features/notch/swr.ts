/**
 * Stale-while-revalidate for the Today reads. Calendar and Reminders are
 * apps on the other end of a script, so a read takes from a few hundred
 * milliseconds to a few seconds; what was last seen is shown at once and the
 * read only corrects it.
 */

type Result<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };

/**
 * `peek` is the last known value (may be slow-ish IPC, never the real read);
 * `fresh` is the real read. The last-known value is applied first unless the
 * real one has already landed, and a failed read leaves what is on screen.
 */
export async function revalidate<T, E>({
  peek,
  fresh,
  onItems,
  onError,
}: {
  peek: () => Promise<T | null | undefined>;
  fresh: () => Promise<Result<T, E>>;
  onItems: (items: T) => void;
  onError: (error: E) => void;
}): Promise<void> {
  let landed = false;
  const warm = peek()
    .then((cached) => {
      if (cached != null && !landed) onItems(cached);
    })
    .catch(() => {});
  const result = await fresh();
  landed = true;
  if (result.status === "ok") onItems(result.data);
  else onError(result.error);
  await warm;
}

/** Last reads by key, for the life of the webview: a view reopened shows its rows on the first frame. */
const memory = new Map<string, unknown>();
export const remembered = <T>(key: string): T | null => (memory.get(key) as T | undefined) ?? null;
export const remember = (key: string, value: unknown): void => void memory.set(key, value);
