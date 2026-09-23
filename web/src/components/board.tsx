import { useMemo } from "react";
import { Background, Controls, Handle, MarkerType, Position, ReactFlow } from "@xyflow/react";
import type { Edge, Node, NodeProps, NodeTypes } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { layoutBoard } from "../lib/board-layout";
import { useMapView } from "../lib/use-map-view";
import { KindGlyph } from "./icons";
import NodeCard from "./node-card";
import Sheet from "./ui/sheet";

interface BoardNodeData extends Record<string, unknown> {
  label: string;
  kindIndex: number;
  multi: boolean;
}

/** A node's box on the board: its name, plus `KindGlyph` when the map
 * declares more than one kind - the same rule `map-tree.tsx` follows,
 * so a node reads the same whether the map is a tree or a canvas. */
function BoardNode({ data }: NodeProps<Node<BoardNodeData>>) {
  return (
    <div className="flex max-w-56 items-center gap-1.5 rounded-md border border-rule bg-panel px-3 py-2 text-[0.8125rem] text-ink">
      <Handle type="target" position={Position.Left} className="!invisible" />
      {data.multi && <KindGlyph index={data.kindIndex} className="size-3 shrink-0 text-faint" />}
      <span className="min-w-0 truncate">{data.label}</span>
      <Handle type="source" position={Position.Right} className="!invisible" />
    </div>
  );
}

const NODE_TYPES: NodeTypes = { board: BoardNode };

/** The whole map on a canvas: pan, zoom, and node dragging - not
 * saved, so a reload always opens on `layoutBoard`'s placement. Tapping
 * a node opens the same `NodeCard` the outline page uses, beside the
 * canvas on a wide screen and as the bottom sheet on a narrow one. */
export default function Board() {
  const { load, map, outline, selected, setNode, close, wide } = useMapView();

  const { nodes, edges } = useMemo(() => {
    if (!map || !outline) return { nodes: [] as Node<BoardNodeData>[], edges: [] as Edge[] };
    const positions = layoutBoard(outline);
    const nodes = map.nodes
      .filter((node) => positions.has(node.id))
      .map(
        (node): Node<BoardNodeData> => ({
          id: node.id,
          type: "board",
          position: positions.get(node.id)!,
          data: {
            label: node.name,
            kindIndex: outline.kindIndex.get(node.kind) ?? 0,
            multi: outline.multi,
          },
        }),
      );
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

  const card = map && outline && selected && (
    <NodeCard id={selected} edges={map.edges} outline={outline} onSelect={setNode} onClose={close} />
  );

  return (
    <main id="board" className="flex min-h-0 flex-1">
      <div className="relative min-w-0 flex-1">
        <div aria-live="polite" className="pointer-events-none absolute left-4 top-4 z-10 text-[0.8125rem] text-faint empty:hidden">
          {load.state === "loading" && <span>Reading the map&#8230;</span>}
          {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
        </div>
        {nodes.length > 0 && (
          // Uncontrolled, so a drag moves a node without the page holding
          // positions it will not keep; the key starts over on a new map.
          <ReactFlow
            key={map?.map.id}
            defaultNodes={nodes}
            defaultEdges={edges}
            nodeTypes={NODE_TYPES}
            nodesConnectable={false}
            onNodeClick={(_event, node) => setNode(node.id)}
            fitView
            // A tall map must still fit whole on open; the default floor
            // of 0.5 would cut its ends off.
            minZoom={0.1}
            proOptions={{ hideAttribution: true }}
          >
            <Background />
            <Controls showInteractive={false} />
          </ReactFlow>
        )}
      </div>

      {card &&
        (wide ? (
          <div className="w-[360px] shrink-0 overflow-auto border-l border-rule p-6">{card}</div>
        ) : (
          <Sheet onClose={close} label={outline?.byId.get(selected)?.name ?? ""}>
            {card}
          </Sheet>
        ))}
    </main>
  );
}
