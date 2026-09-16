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
  /** The project the server is scoped to - stated by the server rather
   * than read off a row, so a filter that matches nothing does not take
   * the page's heading with it. */
  project: string;
}

/** One map a project keeps, as `GET /api/projects` reports it:
 * `headlines` is how many of its nodes a reader sees before opening
 * it, and `gained` how many of those changed since that project's last
 * session. */
export interface ProjectMap {
  name: string;
  headlines: number;
  gained: number;
}

/** One project the log holds events for. `maps` is empty for a project
 * whose root declares no schemas - a checkout deleted since, or one
 * never initialised. */
export interface Project {
  name: string;
  path: string;
  events: number;
  last_active: string;
  maps: ProjectMap[];
}

/** `GET /api/projects`'s body: every project in the log, the most
 * recently active first. */
export interface ProjectsResponse {
  projects: Project[];
}
