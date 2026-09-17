// Typed progress-event subscription. Rust emits the names in
// commands::progress_sequence order (journal order); listeners observe but
// never drive the transaction.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProgressEventName } from "./types";

export const PROGRESS_EVENT = "unscroll-progress";

export interface ProgressPayload {
  event: ProgressEventName;
  serial: string | null;
  detail: string | null;
}

/**
 * Decodes one `unscroll-progress` payload before `normalizeProgressPayload`.
 * Rust emits canonical JSON text (`{"event", "serial", "detail"}`); older
 * builds emitted the bare event name. JSON-parse string payloads and fall
 * back to the bare name (or the original value) when parsing fails — never
 * throw. Non-string payloads (already-structured objects, Tauri wrapping)
 * pass through untouched for the workspace normalizer.
 */
export function decodeProgressPayload(raw: unknown): unknown {
  if (typeof raw !== "string") return raw;
  const trimmed = raw.trim();
  if (!trimmed.startsWith("{")) return raw;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return raw;
  }
}

/**
 * Subscribes to `unscroll-progress`. The decoded payload stays honest:
 * Rust emits JSON text (parsed to {@link ProgressPayload} here) while
 * older builds emitted the bare event name, so handlers receive
 * `string | ProgressPayload` and must narrow before trusting
 * `.event`/`.serial` — typically by passing the payload through
 * `normalizeProgressPayload`. Runtime behavior is decode-then-forward.
 */
export function listenProgress(
  handler: (payload: string | ProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<string | ProgressPayload>(PROGRESS_EVENT, (wrapped) =>
    handler(decodeProgressPayload(wrapped.payload) as string | ProgressPayload),
  );
}
