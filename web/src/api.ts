import type { EventsResponse } from "./types";

/** `GET /api/events`, throwing the status line on a failed fetch - the
 * one place the page reads the log. */
export async function fetchEvents(): Promise<EventsResponse> {
  const response = await fetch("/api/events");
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<EventsResponse>;
}

/** `error`'s message, or its string form when it isn't an `Error` - the
 * one place every `catch` reaches for one. */
export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
