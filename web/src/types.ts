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

/** A node an edge reaches - just enough to link back to it: its short
 * id and name. */
export interface NodeRef {
  id: string;
  name: string;
}

/** One edge touching a row, named by kind and direction, and the node
 * on its other end. */
export interface EdgeRef {
  kind: string;
  dir: "from" | "to";
  node: NodeRef;
}

/** An alternative weighed and lost, folded under the row that answers
 * the same question - or the shape a claim itself carries in common
 * with one. */
export interface OptionRow {
  id: string;
  kind: string;
  name: string;
  why: string | null;
  changed_by: string;
  changed_why: string | null;
  added_at: string;
  changed_at: string;
  sources: Source[];
}

/** One claim in the queue: a headline node changed since the review
 * last opened. */
export interface Row extends OptionRow {
  edges: EdgeRef[];
  related: OptionRow[];
}

/** The headline node a group's rows point at - absent for the orphan
 * group, and for a group whose only row is its own heading. */
export interface Heading {
  id: string;
  title: string;
  raised_at: string;
}

/** Claims that share a heading, or a row with none. */
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

/** `GET /api/review`'s response. */
export interface ReviewResponse {
  maps: MapQueue[];
}
