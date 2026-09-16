/** Who an event is attributed to - the wire shape `GET /api/events`
 * carries, `{"kind":"human","id":"<uuid>"}` or the bare `{"kind":...}`
 * for `agent` and `system`. */
export interface Actor {
  kind: "human" | "agent" | "system";
  id?: string;
}

/** The writer that produced an event and the project it ran in. */
export interface Source {
  name: string;
  path: string;
}

/** One log entry, the shape `percept events search` prints - what
 * `GET /api/events` returns one of, per element of `events`. */
export interface Event {
  id: string;
  actor: Actor;
  source: Source;
  type: string;
  causation_id: string | null;
  created_at: string;
  payload: Record<string, unknown>;
  preview?: { len: number; match?: number };
}

/** `GET /api/events`'s body: the matches, oldest first, how many
 * matched in total before `size` cut them, and `carried` - the folded
 * events (a `tool.resulted`, today) whose `causation_id` names one of
 * `events`. `carried` is never counted in `total` and never a row of
 * its own. */
export interface EventsResponse {
  events: Event[];
  carried: Event[];
  total: number;
}
