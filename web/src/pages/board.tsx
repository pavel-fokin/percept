import { useMemo, useState, type FormEvent } from "react";
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

import { deleteData, describe, getData, patchData, postData, type BoardDetail, type BoardMap, type MapSummary, type Skill, type SkillEntry } from "../api";
import { Button } from "../components/ui/button";
import { usePolling } from "../use-polling";
import type { loadBoard } from "./board-page";

type MapNode = Node<{ title: string; map: string }, "map-node">;

const field = "h-11 w-full rounded-md border border-border bg-bg px-3 text-base text-text placeholder:text-muted focus-visible:border-muted focus-visible:outline-none md:text-sm";

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
  const [activeName, setActiveName] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const active = board.maps.find((m) => m.name === activeName) ?? board.maps[0];
  const selected = board.maps.flatMap((m) => m.nodes.map((n) => ({ map: m.name, node: n }))).find((n) => n.node.id === selectedId);
  const shown = useMemo(() => nodes.map((n) => ({ ...n, selected: n.id === selectedId })), [nodes, selectedId]);

  usePolling(true);

  return (
    <ReactFlow
      // Another set of maps refits the view; a refresh with the same maps keeps where the reader is.
      key={board.maps.map((m) => m.name).join("/")}
      className="flex-1"
      nodes={shown}
      edges={edges}
      onNodeClick={(_, node) => setSelectedId(node.id)}
      onPaneClick={() => setSelectedId(null)}
      nodeTypes={nodeTypes}
      defaultEdgeOptions={edgeOptions}
      nodesDraggable={false}
      nodesConnectable={false}
      fitView
      minZoom={0.1}
      proOptions={{ hideAttribution: true }}
    >
      <Background />
      {board.maps.length === 0 && (
        <div className="pointer-events-none absolute inset-0 z-10 grid place-items-center p-6 text-center">
          <p className="max-w-[36ch] text-muted">Add a map, then draw on it or ask an agent to fill it.</p>
        </div>
      )}
      <Controls position="bottom-right" showInteractive={false} />
      <Panel position="top-right" className="flex flex-col items-end gap-2">
        {active && <MapPicker maps={board.maps} active={active.name} onChange={setActiveName} />}
        {active && <AddNode key={active.name} project={board.project} map={active} />}
        {selected && <EditNode key={selected.node.id} project={board.project} map={selected.map} node={selected.node} onGone={() => setSelectedId(null)} />}
        <AddMap board={board} addable={addable} />
        <RemoveMap board={board} />
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

function MapNodeCard({ data, selected }: NodeProps<MapNode>) {
  return (
    <div
      style={{ width: NODE_WIDTH, height: NODE_HEIGHT }}
      className={`overflow-hidden rounded-md border bg-surface px-3 py-2 text-sm ${selected ? "border-text" : "border-border"}`}
    >
      <Handle type="target" position={Position.Top} className={handle} />
      <p className="truncate font-mono text-xs text-muted">{data.map}</p>
      <p className="line-clamp-2 break-words">{data.title}</p>
      <Handle type="source" position={Position.Bottom} className={handle} />
    </div>
  );
}

function mapApi(project: string, map: string) {
  return `/api/projects/${encodeURIComponent(project)}/maps/${encodeURIComponent(map)}`;
}

/** Chooses the map that "Add node" writes to; with one map there is nothing to choose. */
function MapPicker({ maps, active, onChange }: { maps: BoardMap[]; active: string; onChange: (name: string) => void }) {
  if (maps.length < 2) return null;

  return (
    <select value={active} aria-label="Active map" onChange={(e) => onChange(e.target.value)} className={`${field} w-56`}>
      {maps.map((m) => (
        <option key={m.name} value={m.name}>
          {m.name}
        </option>
      ))}
    </select>
  );
}

function AddNode({ project, map }: { project: string; map: BoardMap }) {
  const { revalidate } = useRevalidator();
  const [open, setOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [typed, setTyped] = useState("");
  const [picked, setPicked] = useState("");
  const [title, setTitle] = useState("");
  const kind = map.kinds.declared ? picked || map.kinds.nodes[0] : typed.trim();

  function toggle() {
    setOpen(!open);
    setFailure(null);
    setTitle("");
  }

  async function create(event: FormEvent) {
    event.preventDefault();
    try {
      await postData<unknown>(`${mapApi(project, map.name)}/nodes`, { kind, title });
      setTitle("");
      setFailure(null);
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  return (
    <div className="relative flex flex-col items-end gap-2">
      <Button variant="secondary" aria-expanded={open} onClick={toggle}>
        Add node
      </Button>
      {open && (
        <form onSubmit={(e) => void create(e)} className="grid w-64 gap-1 rounded-md border border-border bg-surface p-2 text-sm">
          {map.kinds.declared ? (
            <select value={kind} aria-label="Kind" onChange={(e) => setPicked(e.target.value)} className={field}>
              {map.kinds.nodes.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
          ) : (
            <>
              <input
                value={typed}
                list="node-kinds"
                required
                aria-label="Kind"
                placeholder="Kind"
                onChange={(e) => {
                  setTyped(e.target.value);
                  setFailure(null);
                }}
                className={field}
              />
              <datalist id="node-kinds">
                {map.kinds.nodes.map((k) => (
                  <option key={k} value={k} />
                ))}
              </datalist>
            </>
          )}
          <input
            value={title}
            aria-label="Title"
            placeholder="Title"
            onChange={(e) => {
              setTitle(e.target.value);
              setFailure(null);
            }}
            className={field}
          />
          <Button type="submit" variant="secondary" disabled={!kind || !title.trim()} className="h-11">
            Add
          </Button>
          {failure && <p className="px-1 py-1 wrap-anywhere text-muted">{failure}</p>}
        </form>
      )}
    </div>
  );
}

function EditNode({ project, map, node, onGone }: { project: string; map: string; node: { id: string; kind: string; title: string }; onGone: () => void }) {
  const { revalidate } = useRevalidator();
  const [failure, setFailure] = useState<string | null>(null);
  const [title, setTitle] = useState(node.title);
  const url = `${mapApi(project, map)}/nodes/${encodeURIComponent(node.id)}`;

  async function rename(event: FormEvent) {
    event.preventDefault();
    try {
      await patchData<unknown>(url, { title });
      setFailure(null);
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  async function remove() {
    try {
      await deleteData<unknown>(url);
      onGone();
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  return (
    <form onSubmit={(e) => void rename(e)} className="grid w-64 gap-1 rounded-md border border-border bg-surface p-2 text-sm">
      <p className="truncate font-mono text-xs text-muted">{node.kind}</p>
      <input
        value={title}
        aria-label="Node title"
        onChange={(e) => {
          setTitle(e.target.value);
          setFailure(null);
        }}
        className={field}
      />
      <div className="grid grid-cols-2 gap-1">
        <Button type="submit" variant="secondary" disabled={!title.trim() || title === node.title} className="h-11">
          Rename
        </Button>
        <Button type="button" variant="secondary" onClick={() => void remove()} className="h-11">
          Delete
        </Button>
      </div>
      {failure && <p className="px-1 py-1 wrap-anywhere text-muted">{failure}</p>}
    </form>
  );
}

function AddMap({ board, addable }: { board: BoardDetail; addable: MapSummary[] }) {
  const { revalidate } = useRevalidator();
  const [open, setOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [skills, setSkills] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [skill, setSkill] = useState("");

  // The library is read when the panel opens, not on every poll of the board.
  async function toggle() {
    setOpen(!open);
    setFailure(null);
    setName("");
    setSkill("");
    if (open) return;

    try {
      const library = await getData<{ skills: SkillEntry[] }>("/api/skills");
      setSkills(library.skills.filter((s): s is Skill => !("error" in s)).map((s) => s.name));
    } catch (error) {
      setSkills([]);
      setFailure(`skills: ${describe(error)}`);
    }
  }

  async function create(event: FormEvent) {
    event.preventDefault();
    try {
      await postData<BoardDetail>(`/api/projects/${encodeURIComponent(board.project)}/boards/${encodeURIComponent(board.name)}/maps/${encodeURIComponent(name.trim())}`, {
        skill: skill || null,
      });
      setOpen(false);
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  async function add(map: string) {
    try {
      await postData<BoardDetail>(`/api/projects/${encodeURIComponent(board.project)}/boards/${encodeURIComponent(board.name)}/maps`, { map });
      setOpen(false);
      await revalidate();
    } catch (error) {
      setFailure(describe(error));
    }
  }

  return (
    <div className="relative flex flex-col items-end gap-2">
      <Button variant="secondary" aria-expanded={open} onClick={() => void toggle()}>
        Add map
      </Button>
      {open && (
        <div className="w-64 rounded-md border border-border bg-surface p-1 text-sm">
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
          <form onSubmit={(e) => void create(e)} className="mt-1 grid gap-1 border-t border-border px-2 pt-2 pb-1">
            <p className="text-muted">New map</p>
            <input
              value={name}
              aria-label="Map name"
              placeholder="Map name"
              onChange={(e) => {
                setName(e.target.value);
                setFailure(null);
              }}
              className="h-11 w-full rounded-md border border-border bg-bg px-3 text-base text-text placeholder:text-muted focus-visible:border-muted focus-visible:outline-none md:text-sm"
            />
            <select
              value={skill}
              aria-label="Skill"
              onChange={(e) => {
                setSkill(e.target.value);
                setFailure(null);
              }}
              className="h-11 w-full rounded-md border border-border bg-bg px-3 text-base text-text focus-visible:border-muted focus-visible:outline-none md:text-sm"
            >
              <option value="">No skill</option>
              {skills.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
            <Button type="submit" variant="secondary" disabled={!name.trim()} className="h-11">
              Add
            </Button>
          </form>
          {failure && <p className="px-3 py-2 wrap-anywhere text-muted">{failure}</p>}
        </div>
      )}
    </div>
  );
}

function RemoveMap({ board }: { board: BoardDetail }) {
  const { revalidate } = useRevalidator();
  const [open, setOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  function toggle() {
    setOpen(!open);
    setFailure(null);
  }

  async function remove(map: string) {
    try {
      await deleteData<BoardDetail>(`/api/projects/${encodeURIComponent(board.project)}/boards/${encodeURIComponent(board.name)}/maps/${encodeURIComponent(map)}`);
      setOpen(false);
      await revalidate();
    } catch (error) {
      setFailure(`${map}: ${describe(error)}`);
    }
  }

  return (
    <div className="relative flex flex-col items-end gap-2">
      <Button variant="secondary" aria-expanded={open} onClick={toggle}>
        Remove map
      </Button>
      {open && (
        <div className="w-56 rounded-md border border-border bg-surface p-1 text-sm">
          {board.maps.length === 0 && <p className="px-3 py-2 text-muted">No maps on this board.</p>}
          {board.maps.map((map) => (
            <button
              key={map.name}
              type="button"
              onClick={() => void remove(map.name)}
              className="flex min-h-11 w-full cursor-pointer items-center rounded-md px-3 text-left hover:bg-border/50"
            >
              <span className="min-w-0 truncate">{map.name}</span>
            </button>
          ))}
          {failure && <p className="px-3 py-2 wrap-anywhere text-muted">{failure}</p>}
        </div>
      )}
    </div>
  );
}
