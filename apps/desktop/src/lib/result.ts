/**
 * Unwraps a fallible command's result, logging and falling back.
 *
 * Use it where a fallback is genuinely correct — a failed note search should
 * show nothing, not crash the window. Where the user must be told, handle the
 * `{ status: "ok" | "error" }` union directly.
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
