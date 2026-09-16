import type { Event, EventsResponse } from "./types";

/** `GET /api/events`, throwing the status line on a failed fetch - the
 * one place the page reads the log. */
export async function fetchEvents(): Promise<EventsResponse> {
  const response = await fetch("/api/events");
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<EventsResponse>;
}

/** `GET /api/events/{id}` - one event whole, with no preview cut. What
 * a row reaches for when it is opened and the list's copy was shortened.
 */
export async function fetchEvent(id: string): Promise<Event> {
  const response = await fetch(`/api/events/${encodeURIComponent(id)}`);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  const body = (await response.json()) as { event: Event };
  return body.event;
}

/** `error`'s message, or its string form when it isn't an `Error` - the
 * one place every `catch` reaches for one. */
export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
