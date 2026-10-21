import { useRef, useState } from "react";
import {
  Background,
  BaseEdge,
  Controls,
  EdgeLabelRenderer,
  getBezierPath,
  Handle,
  MarkerType,
  Panel,
  Position,
  ReactFlow,
  useReactFlow,
  useStoreApi,
  type EdgeProps,
  type Node,
  type NodeProps,
} from "@xyflow/react";

import { Button } from "../components/ui/button";

type Note = Node<{ text: string }, "note">;

const nodeTypes = { note: NoteNode };

const edgeTypes = { default: LabeledEdge };

const edgeOptions = { markerEnd: { type: MarkerType.ArrowClosed, color: null } };

const handle = "size-4 rounded-full border border-border bg-muted";

function caretToEnd(e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) {
  const end = e.currentTarget.value.length;
  e.currentTarget.setSelectionRange(end, end);
}

export function Board() {
  return (
    <ReactFlow
      className="flex-1"
      defaultNodes={[]}
      defaultEdges={[]}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      nodeOrigin={[0.5, 0]}
      defaultEdgeOptions={edgeOptions}
      proOptions={{ hideAttribution: true }}
    >
      <Background />
      <Controls showInteractive={false} />
      <Panel position="top-left">
        <AddNote />
      </Panel>
    </ReactFlow>
  );
}

function AddNote() {
  const { addNodes } = useReactFlow();
  const store = useStoreApi();
  const count = useRef(0);

  function add() {
    const { width, height, transform: [x, y, zoom] } = store.getState();
    const position = { x: (width / 2 - x) / zoom, y: (height / 2 - y) / zoom };

    addNodes({ id: `note-${count.current++}`, type: "note", position, data: { text: "" } });
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
      <Handle type="target" position={Position.Top} className={handle} />
      {editing ? (
        <textarea
          autoFocus
          onFocus={caretToEnd}
          value={data.text}
          onChange={(e) => updateNodeData(id, { text: e.target.value })}
          onBlur={() => setEditing(false)}
          onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
          className="nodrag nopan nowheel block w-full resize-none bg-transparent field-sizing-content outline-none"
        />
      ) : (
        <p className={`break-words whitespace-pre-wrap ${data.text ? "" : "text-muted"}`}>{data.text || "Empty note"}</p>
      )}
      <Handle type="source" position={Position.Bottom} className={handle} />
    </div>
  );
}

function LabeledEdge({ id, label, markerEnd, ...ends }: EdgeProps) {
  const { updateEdge } = useReactFlow();
  const [editing, setEditing] = useState(false);
  const [path, labelX, labelY] = getBezierPath(ends);

  return (
    <>
      <g onDoubleClick={() => setEditing(true)}>
        <BaseEdge path={path} markerEnd={markerEnd} />
      </g>
      {(editing || label) && (
        <EdgeLabelRenderer>
          <div
            onDoubleClick={() => setEditing(true)}
            style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
            className="nodrag nopan pointer-events-auto absolute rounded-sm bg-bg px-1 text-xs"
          >
            {editing ? (
              <input
                autoFocus
                onFocus={caretToEnd}
                value={typeof label === "string" ? label : ""}
                onChange={(e) => updateEdge(id, { label: e.target.value })}
                onBlur={() => setEditing(false)}
                onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
                className="min-w-16 bg-transparent field-sizing-content outline-none"
              />
            ) : (
              label
            )}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
