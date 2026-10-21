import { useRef, useState } from "react";
import {
  addEdge,
  Background,
  Controls,
  Handle,
  MarkerType,
  Panel,
  Position,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
  useStoreApi,
  type Connection,
  type DefaultEdgeOptions,
  type Node,
  type NodeProps,
  type NodeTypes,
} from "@xyflow/react";

import { Button } from "../components/ui/button";

type Note = Node<{ text: string }, "note">;

const nodeTypes: NodeTypes = { note: NoteNode };

const edgeOptions: DefaultEdgeOptions = { markerEnd: { type: MarkerType.ArrowClosed, color: null } };

export function Board() {
  const [nodes, setNodes, onNodesChange] = useNodesState<Note>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);

  return (
    <div className="relative flex-1">
      <div className="absolute inset-0">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          nodeOrigin={[0.5, 0.5]}
          defaultEdgeOptions={edgeOptions}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={(c: Connection) => setEdges((es) => addEdge(c, es))}
          proOptions={{ hideAttribution: true }}
        >
          <Background />
          <Controls showInteractive={false} />
          <Panel position="top-left">
            <AddNote onAdd={(note) => setNodes((ns) => [...ns, note])} />
          </Panel>
        </ReactFlow>
      </div>
    </div>
  );
}

function AddNote({ onAdd }: { onAdd: (note: Note) => void }) {
  const store = useStoreApi();
  const count = useRef(0);

  function add() {
    const { width, height, transform: [x, y, zoom] } = store.getState();
    const position = { x: (width / 2 - x) / zoom, y: (height / 2 - y) / zoom };

    onAdd({ id: `note-${count.current++}`, type: "note", position, data: { text: "" } });
  }

  return (
    <Button variant="secondary" onClick={add}>
      + Note
    </Button>
  );
}

function NoteNode({ id, data, selected }: NodeProps<Note>) {
  const { updateNodeData } = useReactFlow();
  const [editing, setEditing] = useState(false);
  const ring = selected ? "border-text" : "border-border";

  return (
    <div
      onDoubleClick={() => setEditing(true)}
      className={`min-h-16 w-48 rounded-md border bg-surface p-3 text-sm ${ring}`}
    >
      <Handle type="target" position={Position.Top} className="size-4 rounded-full border border-border bg-muted" />
      {editing ? (
        <textarea
          autoFocus
          value={data.text}
          onChange={(e) => updateNodeData(id, { text: e.target.value })}
          onBlur={() => setEditing(false)}
          onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
          className="nodrag nopan nowheel block w-full resize-none bg-transparent field-sizing-content outline-none"
        />
      ) : (
        <p className={`break-words whitespace-pre-wrap ${data.text ? "" : "text-muted"}`}>{data.text || "Empty note"}</p>
      )}
      <Handle type="source" position={Position.Bottom} className="size-4 rounded-full border border-border bg-muted" />
    </div>
  );
}
