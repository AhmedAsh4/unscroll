// Typed progress-event subscription. Rust emits the names in
// commands::progress_sequence order (journal order); listeners observe but
// never drive the transaction.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProgressEventName } from "./types";

export const PROGRESS_EVENT = "unscroll-progress";

export interface ProgressPayload {
  event: ProgressEventName;
  serial: string;
  detail: string | null;
}

export function listenProgress(
  handler: (payload: ProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<ProgressPayload>(PROGRESS_EVENT, (wrapped) => handler(wrapped.payload));
}
