import type { MapEdge, MapNode } from "./types";

/** A node reached for the first time: its subtree, walked. */
export interface OutlineNode {
  id: string;
  depth: number;
  kids: OutlineEntry[];
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
  /** One tree per head, in the order `nodes` lists them. */
  forest: OutlineNode[];
  /** The heads themselves - what no edge reaches. */
  heads: string[];
  /** Every id with at least one child, real or a ref - what "expand
   * all" opens and what a leaf's dot stands in for. */
  branches: Set<string>;
  /** The edge each id was first reached through, for walking a
   * back-reference's ancestors back up to a head. */
  parent: Map<string, string | null>;
}

/** Folds `nodes` and `edges` into a forest: one tree per head, walked
 * depth-first in edge order, each node named once. A later edge into a
 * node already seen becomes a back-reference instead of repeating its
 * subtree - the same guard that keeps a cycle from looping forever. */
export function buildOutline(nodes: MapNode[], edges: MapEdge[]): Outline {
  const reached = new Set(edges.map((edge) => edge.to));
  const heads = nodes.map((node) => node.id).filter((id) => !reached.has(id));
  const seen = new Set<string>();
  const parent = new Map<string, string | null>();
  const branches = new Set<string>();

  function walk(id: string, depth: number, from: string | null): OutlineEntry {
    if (seen.has(id)) return { ref: true, id, depth };
    seen.add(id);
    parent.set(id, from);
    const out = edges.filter((edge) => edge.from === id);
    if (out.length > 0) branches.add(id);
    const kids = out.map((edge) => walk(edge.to, depth + 1, id));
    return { id, depth, kids };
  }

  const forest = heads.map((head) => walk(head, 0, null) as OutlineNode);
  // A cycle nothing points into has no head; like the CLI's render, one
  // of its own nodes heads it, in add order, so every node is shown.
  for (const node of nodes) {
    if (seen.has(node.id)) continue;
    heads.push(node.id);
    forest.push(walk(node.id, 0, null) as OutlineNode);
  }
  return { forest, heads, branches, parent };
}

/** How many real nodes hang under `node`, not counting back-references -
 * what a collapsed branch's row states so a reader can judge its size
 * without opening it. */
export function subtreeSize(node: OutlineNode): number {
  return node.kids.reduce((total, kid) => total + ("ref" in kid ? 0 : 1 + subtreeSize(kid)), 0);
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
