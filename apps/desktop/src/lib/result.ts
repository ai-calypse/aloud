/**
 * Unwraps a fallible command's result.
 *
 * Rust commands that return `Result` cross the boundary as a
 * `{ status: "ok" | "error" }` union, so every call site has to handle the
 * error arm. Most of them have nothing useful to do with it — a failed note
 * search should log and show nothing, not crash the window — so this logs and
 * falls back.
 *
 * Use it where a fallback is genuinely correct. Where the user must be told,
 * handle the union directly.
 */
export async function or<T>(
  promise: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>,
  fallback: T,
  context: string,
): Promise<T> {
  const result = await promise;
  if (result.status === "ok") return result.data;

  console.error(`${context}: ${result.error}`);
  return fallback;
}
