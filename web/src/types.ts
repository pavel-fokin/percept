/** A node's standing, as `Map::standing` reports it. */
export type Standing = "claimed" | "seen" | "confirmed" | "disputed";

/** A node this one supersedes, or one it reopens - just enough to link
 * back to it: its short id and name. */
export interface NodeRef {
  id: string;
  name: string;
}

/** An alternative weighed and lost, folded under the row that answers
 * the same question. */
export interface Option {
  id: string;
  kind: string;
  name: string;
  why: string | null;
  standing: Standing;
  dispute: string | null;
}

/** One claim in the queue: a headline node the map's fold marks
 * `claimed`, or judged since the map was last finished. */
export interface Claim extends Option {
  added_at: string;
  was: NodeRef | null;
  reopens: NodeRef[];
  options: Option[];
}

/** Claims that share a settlement question, or a task with none. */
export interface Group {
  id: string;
  title: string;
  raised_at: string;
  claims: Claim[];
}

/** One map's queue: its claims, grouped by the question or task each
 * answers. */
export interface MapQueue {
  name: string;
  purpose: string;
  since: string | null;
  groups: Group[];
}

/** `GET /api/review`'s response. */
export interface ReviewResponse {
  maps: MapQueue[];
}
