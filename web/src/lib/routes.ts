/** Where each page lives, and how a link to one is written. Paths
 * rather than hashes: the log's filters and a map's cut both live in
 * the query string, so `/log?q=drift` and `/maps/decisions?root=…` are
 * links the server and the page read the same way, where a hash would
 * have to carry a query string of its own. */

export const PATHS = {
  index: "/",
  log: "/log",
  map: "/maps/:id",
  board: "/board",
} as const;

/** One project's map, with the root it belongs to. `id` is the map's
 * own id, not its schema name - two projects can declare the same
 * schema. The root is a filesystem path and the only unambiguous name
 * a project has - two checkouts can share a basename - so it rides
 * the query string beside the cut. */
export function mapPath(id: string, root: string): string {
  return `/maps/${encodeURIComponent(id)}?root=${encodeURIComponent(root)}`;
}

/** The same map, laid out on a canvas rather than as an outline. A
 * page of its own, not a view nested under `/maps/:id`: a board will
 * later hold several maps as its own entity. */
export function boardPath(id: string, root: string): string {
  return `/board?map=${encodeURIComponent(id)}&root=${encodeURIComponent(root)}`;
}

/** The log's search field, named once: the field wears it as its `id`,
 * the label points at it, and a link that means "search the log"
 * carries it as a fragment. */
export const SEARCH_FIELD_ID = "q";
export const SEARCH_HASH = `#${SEARCH_FIELD_ID}`;
