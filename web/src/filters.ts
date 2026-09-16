/** The Everything screen's filter: pure derivations only, so the search
 * box, the two `<details>` menus, and the URL each read the same state
 * through a function rather than repeating its shape. */

/** One choice in the kind menu - a row kind, never an event kind:
 * `tool.resulted` folds into the call that caused it, so it is never
 * offered here. Order is the order the menu lists them in. */
export interface KindOption {
  value: string;
  label: string;
}

export const KIND_OPTIONS: KindOption[] = [
  { value: "message.received", label: "Messages" },
  { value: "thought.recorded", label: "Thoughts" },
  { value: "tool.called", label: "Tool calls" },
  { value: "node.added", label: "Nodes added" },
  { value: "node.changed", label: "Nodes changed" },
  { value: "node.removed", label: "Nodes removed" },
  { value: "edge.added", label: "Links added" },
  { value: "edge.removed", label: "Links removed" },
  { value: "model.called", label: "Model calls" },
  { value: "session.started", label: "Sessions" },
  { value: "file.cited", label: "Citations" },
];

/** One choice in the time menu - `value` is the `since` shorthand
 * `GET /api/events` reads, `null` for "no bound at all". */
export interface TimeOption {
  value: string | null;
  label: string;
}

export const TIME_OPTIONS: TimeOption[] = [
  { value: null, label: "Any time" },
  { value: "1h", label: "Last hour" },
  { value: "1d", label: "Last 24 hours" },
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
];

/** One choice in the actor menu - the server's `store::parse_actor`
 * words. Order is the order the menu lists them in. */
export interface ActorOption {
  value: string;
  label: string;
}

export const ACTOR_OPTIONS: ActorOption[] = [
  { value: "human", label: "You" },
  { value: "agent", label: "Agents" },
  { value: "system", label: "percept" },
];

/** The Everything screen's whole filter state - held once, in `App`,
 * and passed down rather than duplicated in the components that read
 * or change one piece of it. */
export interface Filter {
  q: string;
  kinds: string[];
  actors: string[];
  since: string | null;
}

export const EMPTY_FILTER: Filter = { q: "", kinds: [], actors: [], since: null };

export function isFilterActive(filter: Filter): boolean {
  return filter.q !== "" || filter.kinds.length > 0 || filter.actors.length > 0 || filter.since !== null;
}

/** `kinds`, with its selection of `type` toggled - the one place a
 * click on a kind row turns into the next array. */
export function toggleKind(kinds: string[], type: string): string[] {
  return kinds.includes(type) ? kinds.filter((kind) => kind !== type) : [...kinds, type];
}

/** `actors`, with its selection of `actor` toggled - the actor menu's
 * own version of `toggleKind`. */
export function toggleActor(actors: string[], actor: string): string[] {
  return actors.includes(actor) ? actors.filter((a) => a !== actor) : [...actors, actor];
}

/** The kind chip's label - its own state, not a fixed word: none
 * picked reads as "all", one picked names it, several count. */
export function kindsLabel(kinds: string[]): string {
  if (kinds.length === 0) return "All kinds";
  if (kinds.length === 1) {
    return KIND_OPTIONS.find((option) => option.value === kinds[0])?.label ?? kinds[0];
  }
  return `${kinds.length} kinds`;
}

/** The time chip's label - the picked option's own words. */
export function timeLabel(since: string | null): string {
  return TIME_OPTIONS.find((option) => option.value === since)?.label ?? "All time";
}

/** The actor chip's label - the same none/one/several rule as
 * `kindsLabel`. */
export function actorsLabel(actors: string[]): string {
  if (actors.length === 0) return "All actors";
  if (actors.length === 1) {
    return ACTOR_OPTIONS.find((option) => option.value === actors[0])?.label ?? actors[0];
  }
  return `${actors.length} actors`;
}

/** `filter`, read from the page's URL - so a filtered view is a link
 * that still shows the same filter after a reload. */
export function filterFromSearch(search: string): Filter {
  const params = new URLSearchParams(search);
  const type = params.get("type");
  const actor = params.get("actor");
  return {
    q: params.get("q") ?? "",
    kinds: type ? type.split(",").filter(Boolean) : [],
    actors: actor ? actor.split(",").filter(Boolean) : [],
    since: params.get("since"),
  };
}

/** `filter`, written back as a query string - `?q=`, `?type=`,
 * `?actor=`, `?since=`, each left out when unset, so an unfiltered view
 * carries no query string at all. */
export function searchFromFilter(filter: Filter): string {
  const params = new URLSearchParams();
  if (filter.q) params.set("q", filter.q);
  if (filter.kinds.length > 0) params.set("type", filter.kinds.join(","));
  if (filter.actors.length > 0) params.set("actor", filter.actors.join(","));
  if (filter.since) params.set("since", filter.since);
  const query = params.toString();
  return query ? `?${query}` : "";
}
