/** A node's standing, as `Map::standing` reports it. */
export type Standing = "claimed" | "seen" | "confirmed" | "disputed";

/** The proposal a human prompt's "yes" answered: the latest agent
 * reply in the same source before it, or `null` when there is none. */
export interface Proposal {
  id: string;
  at: string;
  content: string;
}

/** One event a node's `sources` names, as `GET /api/review` resolves
 * it - what the source line folds open to. */
export type Source =
  | {
      kind: "message";
      actor: "human" | "agent";
      id: string;
      at: string;
      client: string;
      content: string;
      truncated: boolean;
      proposal: Proposal | null;
    }
  | {
      kind: "file";
      id: string;
      at: string;
      path: string;
      lines: [number, number] | null;
      label: string;
      excerpt: string;
      truncated: boolean;
    }
  | { kind: "event"; id: string; at: string; type: string }
  | { kind: "missing"; id: string };

/** A node this one supersedes, or one it reopens - just enough to link
 * back to it: its short id and name. */
export interface NodeRef {
  id: string;
  name: string;
}

/** An alternative weighed and lost, folded under the row that answers
 * the same question - or the shape a claim itself carries in common
 * with one. */
export interface OptionRow {
  id: string;
  kind: string;
  name: string;
  why: string | null;
  standing: Standing;
  dispute: string | null;
  sources: Source[];
}

/** One claim in the queue: a headline node the map's fold marks
 * `claimed`, or judged since the map was last finished. */
export interface Row extends OptionRow {
  added_at: string;
  was: NodeRef | null;
  reopens: NodeRef[];
  options: OptionRow[];
}

/** The question or task a group's rows answer - absent for the orphan
 * group, and for a group whose only row is its own question. */
export interface Heading {
  id: string;
  title: string;
  raised_at: string;
}

/** Claims that share a settlement question, or a task with none. */
export interface Group {
  heading: Heading | null;
  claims: Row[];
}

/** One map's queue: its claims, grouped by the question or task each
 * answers. */
export interface MapQueue {
  name: string;
  purpose: string;
  since: string | null;
  groups: Group[];
}

/** `GET /api/review`'s response. `next` is the lines the next session's
 * start block will print - the same text `judged_since_block` builds
 * for the hook - or `null` when nothing was judged since the project's
 * last session. */
export interface ReviewResponse {
  maps: MapQueue[];
  next: string | null;
}
