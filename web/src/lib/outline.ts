import type { MapEdge, MapNode, MapResponse } from "./types";

/** A node reached for the first time: its subtree, walked. `size` is
 * how many real nodes hang under it, not counting back-references -
 * what a collapsed branch's row states so a reader can judge its size
 * without opening it. */
export interface OutlineNode {
  id: string;
  depth: number;
  kids: OutlineEntry[];
  size: number;
}

/** A node reached again: a back-reference row, no subtree of its own -
 * the map is a graph, but a node is named once. */
export interface OutlineRef {
  ref: true;
  id: string;
  depth: number;
}

export type OutlineEntry = OutlineNode | OutlineRef;

export interface Outline {
  /** One tree per head, in the map's own head order, an unreached
   * cycle's own node last, in `nodes` order. */
  forest: OutlineNode[];
  /** Every id with at least one child, real or a ref - what "expand
   * all" opens and what a leaf's dot stands in for. */
  branches: Set<string>;
  /** The edge each id was first reached through, for walking a
   * back-reference's ancestors back up to a head. */
  parent: Map<string, string | null>;
  /** Every node by its short id, built once for every lookup a
   * renderer needs. */
  byId: Map<string, MapNode>;
  /** Every outgoing edge, by its `from` id, built once so a walk never
   * filters the whole edge list per node. */
  children: Map<string, MapEdge[]>;
  /** A kind's index in `map.kinds`, for `KindGlyph`. */
  kindIndex: Map<string, number>;
  /** Whether the map declares more than one node kind - a legend and
   * per-row glyphs only earn their place when it does. */
  multi: boolean;
}

/** Folds a `MapResponse` into a forest: one tree per head, walked
 * depth-first in edge order, each node named once. A later edge into a
 * node already seen becomes a back-reference instead of repeating its
 * subtree - the same guard that keeps a cycle from looping forever. */
export function buildOutline(map: MapResponse): Outline {
  const byId = new Map(map.nodes.map((node) => [node.id, node]));
  const children = new Map<string, MapEdge[]>();
  for (const edge of map.edges) {
    const list = children.get(edge.from);
    if (list) list.push(edge);
    else children.set(edge.from, [edge]);
  }
  const kindIndex = new Map(map.kinds.map((kind, index) => [kind.kind, index]));
  const multi = map.kinds.length > 1;

  const seen = new Set<string>();
  const parent = new Map<string, string | null>();
  const branches = new Set<string>();

  function walk(id: string, depth: number, from: string | null): OutlineEntry {
    if (seen.has(id)) return { ref: true, id, depth };
    seen.add(id);
    parent.set(id, from);
    const out = children.get(id) ?? [];
    if (out.length > 0) branches.add(id);
    const kids = out.map((edge) => walk(edge.to, depth + 1, id));
    const size = kids.reduce((total, kid) => total + ("ref" in kid ? 0 : 1 + kid.size), 0);
    return { id, depth, kids, size };
  }

  const forest = map.heads.map((head) => walk(head, 0, null) as OutlineNode);
  // A cycle nothing points into has no head; like the CLI's render, one
  // of its own nodes heads it, in add order, so every node is shown.
  for (const node of map.nodes) {
    if (seen.has(node.id)) continue;
    forest.push(walk(node.id, 0, null) as OutlineNode);
  }
  return { forest, branches, parent, byId, children, kindIndex, multi };
}

/** `id`'s ancestors, head first, `id` last - the path a collapsed
 * branch must open to bring a back-reference's real row into view. */
export function pathTo(id: string, parent: Map<string, string | null>): string[] {
  const path: string[] = [];
  for (let current: string | null = id; current !== null; current = parent.get(current) ?? null) {
    path.unshift(current);
  }
  return path;
}
