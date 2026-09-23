import { useCallback, useMemo } from "react";
import { Background, Controls, Handle, MarkerType, Position, ReactFlow } from "@xyflow/react";
import type { Edge, Node, NodeMouseHandler, NodeProps, NodeTypes } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useSearchParams } from "react-router";
import { useMapView } from "../hooks/use-map-view";
import { layoutBoard } from "../lib/board-layout";
import { KindGlyph } from "./icons";
import { MapStatus, SelectedCard } from "./map-chrome";

interface BoardNodeData extends Record<string, unknown> {
  label: string;
  /** The kind's glyph index, or null when the map declares one kind. */
  kindIndex: number | null;
}

/** A node's box on the board: its name, plus `KindGlyph` when the map
 * declares more than one kind - the same rule `map-tree.tsx` follows,
 * so a node reads the same whether the map is a tree or a canvas. */
function BoardNode({ data }: NodeProps<Node<BoardNodeData>>) {
  return (
    <div className="flex max-w-56 items-center gap-1.5 rounded-md border border-rule bg-panel px-3 py-2 text-[0.8125rem] text-ink">
      <Handle type="target" position={Position.Left} className="!invisible" />
      {data.kindIndex !== null && <KindGlyph index={data.kindIndex} className="size-3 shrink-0 text-faint" />}
      <span className="min-w-0 truncate">{data.label}</span>
      <Handle type="source" position={Position.Right} className="!invisible" />
    </div>
  );
}

const NODE_TYPES: NodeTypes = { board: BoardNode };
const PRO_OPTIONS = { hideAttribution: true };

/** The whole map on a canvas: pan, zoom, and node dragging - not
 * saved, so a reload always opens on `layoutBoard`'s placement. Tapping
 * a node opens the same `NodeCard` the outline page uses, beside the
 * canvas on a wide screen and as the bottom sheet on a narrow one. */
export default function Board() {
  const [params] = useSearchParams();
  const view = useMapView(params.get("map") ?? "");
  const { load, map, outline, setNode } = view;

  const flow = useMemo(() => {
    if (!map || !outline) return null;
    const positions = layoutBoard(outline);
    const nodes = map.nodes.flatMap((node): Node<BoardNodeData>[] => {
      const position = positions.get(node.id);
      if (!position) return [];
      const kindIndex = outline.multi ? (outline.kindIndex.get(node.kind) ?? 0) : null;
      return [{ id: node.id, type: "board", position, data: { label: node.name, kindIndex } }];
    });
    const edges = map.edges.map(
      (edge, index): Edge => ({
        id: `${edge.from}-${edge.to}-${index}`,
        source: edge.from,
        target: edge.to,
        markerEnd: { type: MarkerType.ArrowClosed },
      }),
    );
    return { nodes, edges };
  }, [map, outline]);

  const onNodeClick: NodeMouseHandler = useCallback((_event, node) => setNode(node.id), [setNode]);

  // One absolute row inside the shell's leftover space, so the canvas
  // and the card get a definite height without knowing how tall the
  // header is, and a long card scrolls rather than growing the page.
  return (
    <main id="board" className="relative flex-1">
      <div className="absolute inset-0 flex">
        <div className="relative min-w-0 flex-1">
          <MapStatus load={load} className="pointer-events-none absolute left-4 top-4 z-10" />
          {flow && (
            // Uncontrolled, so a drag moves a node without the page holding
            // positions it will not keep; the key starts over on a new map.
            <ReactFlow
              key={map?.map.id}
              defaultNodes={flow.nodes}
              defaultEdges={flow.edges}
              nodeTypes={NODE_TYPES}
              nodesConnectable={false}
              onNodeClick={onNodeClick}
              fitView
              // A tall map must still fit whole on open; the default floor
              // of 0.5 would cut its ends off.
              minZoom={0.1}
              proOptions={PRO_OPTIONS}
            >
              <Background />
              <Controls showInteractive={false} />
            </ReactFlow>
          )}
        </div>
        <SelectedCard view={view} wideClassName="w-[360px] shrink-0 overflow-auto border-l border-rule p-6" />
      </div>
    </main>
  );
}
