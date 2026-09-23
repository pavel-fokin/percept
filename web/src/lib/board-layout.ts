import type { Outline, OutlineEntry } from "./outline";

export interface BoardPosition {
  x: number;
  y: number;
}

const COLUMN_WIDTH = 240;
const ROW_HEIGHT = 52;

/** One position per node: depth is the column, the depth-first walk
 * order from `buildOutline` is the row. A node is placed once, the
 * first time the walk reaches it - a later edge into it draws as a
 * board edge, not a second box. Kept a standalone function, not a
 * method on `Outline`, because a saved layout will replace it without
 * either caller changing. */
export function layoutBoard(outline: Outline): Map<string, BoardPosition> {
  const positions = new Map<string, BoardPosition>();
  let row = 0;

  function walk(entry: OutlineEntry) {
    if ("ref" in entry) return;
    positions.set(entry.id, { x: entry.depth * COLUMN_WIDTH, y: row * ROW_HEIGHT });
    row += 1;
    for (const kid of entry.kids) walk(kid);
  }

  for (const head of outline.forest) walk(head);
  return positions;
}
