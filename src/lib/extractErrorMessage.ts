/**
 * Extract a human-readable error message from Tauri IPC errors.
 *
 * Tauri serializes Rust enums (like AppError) as tagged objects:
 *   { "LlmService": "some message" }
 *   { "FFmpeg": "ffmpeg not found" }
 *
 * JavaScript's `String(obj)` produces "[object Object]" for these.
 * This helper extracts the actual message string.
 */
export function extractErrorMessage(e: unknown): string {
  if (typeof e === 'string') return e;

  if (typeof e === 'object' && e !== null) {
    // Tauri AppError serialized as { "VariantName": "message" }
    const values = Object.values(e as Record<string, unknown>);
    if (values.length > 0 && typeof values[0] === 'string') {
      return values[0];
    }
    // Error instance
    if ('message' in e && typeof (e as Error).message === 'string') {
      return (e as Error).message;
    }
    return JSON.stringify(e);
  }

  return String(e);
}
