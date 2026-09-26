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

/** One log entry, the shape `percept search` prints - what
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

/** One map a project keeps, as `GET /api/projects` reports it: `id` is
 * the map's own stable id - what `GET /api/maps/{id}` addresses it
 * by - `name` its schema's, for display only; `nodes` is how many it
 * holds, and `gained` how many of those changed since that project's
 * last session. */
export interface ProjectMap {
  id: string;
  name: string;
  nodes: number;
  gained: number;
}

/** One project the log holds events for. `maps` is empty for a project
 * whose root declares no schemas - a checkout deleted since, or one
 * never initialised - and for one whose schemas could not be read, where
 * `maps_error` says why. */
export interface Project {
  name: string;
  path: string;
  events: number;
  last_active: string;
  maps: ProjectMap[];
  maps_error: string | null;
}

/** `GET /api/projects`'s body: every project in the log, the most
 * recently active first. */
export interface ProjectsResponse {
  projects: Project[];
}

/** One node kind the schema declares - its short id prefix, so a page
 * can name a kind in the schema's language and never in its own. */
export interface MapKind {
  kind: string;
  prefix: string;
}

/** One node of a map: `id` is the map's own short id (`c1`, `q12`),
 * `node` the uuid. An edge names the short ids, so the page joins on
 * what it displays. */
export interface MapNode {
  id: string;
  node: string;
  kind: string;
  name: string;
  properties: Record<string, string>;
}

export interface MapEdge {
  kind: string;
  from: string;
  to: string;
}

/** `GET /api/maps/{id}`'s body: one map, whole - every node and edge
 * it folds to. `heads` is the nodes no edge reaches, in the one order
 * the server and the CLI's render agree on. */
export interface MapResponse {
  map: { id: string; name: string; purpose: string };
  kinds: MapKind[];
  nodes: MapNode[];
  edges: MapEdge[];
  heads: string[];
  project: string;
}
