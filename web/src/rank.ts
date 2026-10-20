import type { MapEdge, MapNode } from "./api";

/**
 * Nodes grouped by rank, highest first, each group sorted by title.
 * A node's rank is its height: the longest path along edges, of any kind, from it to a node with no outgoing edges.
 */
export function byRank(nodes: MapNode[], edges: MapEdge[]) {
  const out = new Map<string, string[]>();
  for (const e of edges) out.set(e.from, [...(out.get(e.from) ?? []), e.to]);

  const heights = new Map<string, number>();
  const visiting = new Set<string>();
  // An edge back into a node still being visited closes a cycle and counts as nothing.
  const height = (id: string): number => {
    const known = heights.get(id);
    if (known !== undefined) return known;

    visiting.add(id);
    let tallest = 0;
    for (const to of out.get(id) ?? []) {
      if (!visiting.has(to)) tallest = Math.max(tallest, height(to) + 1);
    }
    visiting.delete(id);

    heights.set(id, tallest);
    return tallest;
  };

  const ranks = new Map<number, MapNode[]>();
  for (const node of nodes) {
    const rank = height(node.id);
    ranks.set(rank, [...(ranks.get(rank) ?? []), node]);
  }

  return [...ranks]
    .sort(([a], [b]) => b - a)
    .map(([, group]) => group.sort((a, b) => a.title.localeCompare(b.title)));
}
