import type { Filter } from "./filters";
import type { Event, EventsResponse } from "./types";

/** `GET /api/events` - the log's most recent window matching `filter`.
 * `until` is exclusive, so passing the oldest event already held asks
 * for the window before it, with no overlap and no gap. */
export async function fetchEvents(filter: Filter, until?: string): Promise<EventsResponse> {
  const params = new URLSearchParams();
  if (filter.q) params.set("contains", filter.q);
  if (filter.kinds.length > 0) params.set("type", filter.kinds.join(","));
  if (filter.actors.length > 0) params.set("actor", filter.actors.join(","));
  if (filter.since) params.set("since", filter.since);
  if (until) params.set("until", until);
  const query = params.toString();
  const response = await fetch(`/api/events${query ? `?${query}` : ""}`);
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
