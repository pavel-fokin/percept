import { useMemo, useState } from "react";
import dagre from "@dagrejs/dagre";
import {
  Background,
  Controls,
  Handle,
  MarkerType,
  Panel,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import { useLoaderData, useRevalidator } from "react-router";

import { describe, postData, type BoardDetail, type MapSummary } from "../api";
import { Button } from "../components/ui/button";
import { usePolling } from "../use-polling";
import type { loadBoard } from "./board-page";

type MapNode = Node<{ title: string; map: string }, "map-node">;

const nodeTypes = { "map-node": MapNodeCard };

const edgeOptions = { markerEnd: { type: MarkerType.ArrowClosed, color: null } };

const labelStyle = { fill: "var(--color-muted)" };

const labelBgStyle = { fill: "var(--color-bg)" };

const handle = "size-1 border-0 opacity-0";

const NODE_WIDTH = 192;
const NODE_HEIGHT = 64;

export function Board() {
  const { board, addable } = useLoaderData<typeof loadBoard>();
  const { nodes, edges } = useMemo(() => layout(board), [board]);

  usePolling(true);

  return (
    <ReactFlow
      // Another set of maps refits the view; a refresh with the same maps keeps where the reader is.
      key={board.maps.map((m) => m.name).join("/")}
      className="flex-1"
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      defaultEdgeOptions={edgeOptions}
      nodesDraggable={false}
      nodesConnectable={false}
      fitView
      minZoom={0.1}
      proOptions={{ hideAttribution: true }}
    >
      <Background />
      <Controls position="bottom-right" showInteractive={false} />
      <Panel position="top-right">
        <AddMap board={board} addable={addable} />
      </Panel>
    </ReactFlow>
  );
}

/** The nodes and edges of every map on the board, placed top to bottom by dagre. */
function layout(board: BoardDetail) {
  const graph = new dagre.graphlib.Graph();
  graph.setGraph({ rankdir: "TB", nodesep: 40, ranksep: 80 });
  graph.setDefaultEdgeLabel(() => ({}));

  for (const map of board.maps) {
    for (const node of map.nodes) graph.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
    for (const edge of map.edges) graph.setEdge(edge.from, edge.to);
  }
  dagre.layout(graph);

  const nodes = board.maps.flatMap((map) =>
    map.nodes.map(
      (node): MapNode => {
        const { x, y } = graph.node(node.id);
        return {
          id: node.id,
          type: "map-node",
          position: { x: x - NODE_WIDTH / 2, y: y - NODE_HEIGHT / 2 },
          data: { title: node.title, map: map.name },
        };
      },
    ),
  );
  const edges = board.maps.flatMap((map) =>
    map.edges.map((edge): Edge => ({ id: `${edge.from}-${edge.kind}-${edge.to}`, source: edge.from, target: edge.to, label: edge.kind, labelStyle, labelBgStyle })),
  );

  return { nodes, edges };
}

function MapNodeCard({ data }: NodeProps<MapNode>) {
  return (
    <div style={{ width: NODE_WIDTH, height: NODE_HEIGHT }} className="overflow-hidden rounded-md border border-border bg-surface px-3 py-2 text-sm">
      <Handle type="target" position={Position.Top} className={handle} />
      <p className="truncate font-mono text-xs text-muted">{data.map}</p>
      <p className="line-clamp-2 break-words">{data.title}</p>
      <Handle type="source" position={Position.Bottom} className={handle} />
    </div>
  );
}

function AddMap({ board, addable }: { board: BoardDetail; addable: MapSummary[] }) {
  const { revalidate } = useRevalidator();
  const [open, setOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  async function add(map: string) {
    try {
      await postData<BoardDetail>(`/api/projects/${encodeURIComponent(board.project)}/boards/${encodeURIComponent(board.name)}/maps`, { map });
      setOpen(false);
      setFailure(null);
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  return (
    <div className="relative flex flex-col items-end gap-2">
      <Button variant="secondary" aria-expanded={open} onClick={() => setOpen(!open)}>
        Add map
      </Button>
      {open && (
        <div className="w-56 rounded-md border border-border bg-surface p-1 text-sm">
          {addable.length === 0 && <p className="px-3 py-2 text-muted">No other maps in {board.project}.</p>}
          {addable.map((map) => (
            <button
              key={map.name}
              type="button"
              onClick={() => void add(map.name)}
              className="flex min-h-11 w-full cursor-pointer items-center justify-between gap-3 rounded-md px-3 text-left hover:bg-border/50"
            >
              <span className="min-w-0 truncate">{map.name}</span>
              <span className="text-muted tabular-nums">{map.nodes}</span>
            </button>
          ))}
          {failure && <p className="px-3 py-2 wrap-anywhere text-muted">{failure}</p>}
        </div>
      )}
    </div>
  );
}
