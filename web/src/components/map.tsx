import { useEffect, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { fetchMap, messageOf } from "../lib/api";
import FilterMenu from "./filter-menu";
import type { FilterMenuOption } from "./filter-menu";
import { basename, count } from "../lib/format";
import { mapPath } from "../lib/routes";
import type { MapEdge, MapNode, MapResponse } from "../lib/types";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; cut: MapResponse };

const DEFAULT_DEPTH = 1;

/** How far out a cut reaches. Zero is the node alone, which is what a
 * reader asks for when they want one node's own words with nothing
 * around it. Past three the cut stops being a cut. */
const DEPTH_OPTIONS: FilterMenuOption<string>[] = [
  { value: "0", label: "This node alone" },
  { value: "1", label: "One step out" },
  { value: "2", label: "Two steps out" },
  { value: "3", label: "Three steps out" },
];

/** One cognitive map, cut. It opens on the overview - the nodes that
 * head the map, the level a reader can hold - and every node id is a link
 * that re-cuts around it, which is the position the CLI's `--around`
 * asks a reader to already know. */
export default function MapView() {
  const { id = "" } = useParams();
  const [params] = useSearchParams();
  const root = params.get("root") ?? "";
  const around = params.get("around");
  const depth = readDepth(params.get("depth"));
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [depthOpen, setDepthOpen] = useState(false);
  const navigate = useNavigate();

  useEffect(() => {
    let cancelled = false;
    setLoad({ state: "loading" });
    fetchMap(id, root, around, depth)
      .then((cut) => {
        if (!cancelled) setLoad({ state: "ready", cut });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoad({ state: "failed", message: messageOf(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [id, root, around, depth]);

  const cut = load.state === "ready" ? load.cut : null;
  const name = cut?.map.name ?? "";

  return (
    <main id="map" className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
        <h1 className="text-base font-medium tracking-tight text-ink">{name}</h1>
        {root && <p className="text-[0.8125rem] text-faint">&#183; from {basename(root)}</p>}
      </div>
      {cut && <p className="mt-1 font-serif text-[1.0625rem] leading-relaxed text-muted">{cut.map.purpose}</p>}

      {around && (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <FilterMenu
            chipLabel={DEPTH_OPTIONS.find((option) => option.value === String(depth))?.label ?? `${depth} steps out`}
            options={DEPTH_OPTIONS}
            isSelected={(value) => value === String(depth)}
            onSelect={(value) => navigate(`${mapPath(id, root)}&around=${encodeURIComponent(around)}&depth=${value}`)}
            multi={false}
            open={depthOpen}
            onOpenChange={setDepthOpen}
          />
        </div>
      )}

      <div aria-live="polite" className="mt-3 flex min-h-6 flex-wrap items-center gap-x-3 text-[0.8125rem] text-faint">
        {load.state === "loading" && <span>Reading the map&#8230;</span>}
        {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
        {cut && (
          <>
            <span>{bound(cut, around)}</span>
            {around && (
              <Link to={mapPath(id, root)} className="text-accent underline decoration-rule underline-offset-4 hover:decoration-faint">
                Show the whole map
              </Link>
            )}
          </>
        )}
      </div>

      {cut && cut.nodes.length > 0 && (
        <ul className="mt-4">
          {cut.nodes.map((node, index, all) => (
            <li
              key={node.node}
              className={"border-t border-rule py-3" + (index === all.length - 1 ? " border-b" : "")}
            >
              <NodeRow node={node} cut={cut} mapId={id} root={root} around={around} />
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}

/** `depth` from the address. Zero is a real answer - the node alone -
 * so this cannot fall back on a falsy test. */
function readDepth(raw: string | null): number {
  if (raw === null) return DEFAULT_DEPTH;
  const depth = Number(raw);
  return Number.isInteger(depth) && depth >= 0 ? depth : DEFAULT_DEPTH;
}

/** What the cut holds, and what it left out - a lens that does not say
 * what it left out is not finished. */
function bound(cut: MapResponse, around: string | null): string {
  const shown = `${count(cut.shown_nodes)} of ${count(cut.total_nodes)} nodes`;
  if (!around) return `${shown}, what heads this map.`;
  const beyond = cut.boundary_edges;
  const out = beyond === 0 ? "" : `, with ${count(beyond)} ${beyond === 1 ? "link" : "links"} out of it`;
  return `${shown}, around ${around}${out}.`;
}

function NodeRow({
  node,
  cut,
  mapId,
  root,
  around,
}: {
  node: MapNode;
  cut: MapResponse;
  mapId: string;
  root: string;
  around: string | null;
}) {
  const here = node.id === around;

  return (
    <>
      <div className="flex flex-wrap items-baseline gap-x-2 text-xs">
        {here ? (
          <span className="font-mono text-accent">{node.id}</span>
        ) : (
          <Link
            to={`${mapPath(mapId, root)}&around=${encodeURIComponent(node.id)}&depth=${DEFAULT_DEPTH}`}
            className="font-mono text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
          >
            {node.id}
          </Link>
        )}
        <span className="text-faint">{node.kind}</span>
      </div>
      <p className="mt-0.5 text-[0.9375rem] leading-relaxed text-ink">{node.name}</p>
      {Object.entries(node.properties).map(([key, value]) => (
        <p key={key} className="mt-1 text-[0.8125rem] leading-relaxed text-muted">
          <span className="text-faint">{key} </span>
          {value}
        </p>
      ))}
      <Links node={node} edges={cut.edges} />
    </>
  );
}

/** The edges this node is an end of, in the cut. The arrow carries the
 * direction and the far end is named after it, so `→ about c1` reads
 * "this is about c1" and `← about q1` reads "q1 is about this". A
 * relationship a reader cannot orient is half a relationship. */
function Links({ node, edges }: { node: MapNode; edges: MapEdge[] }) {
  const links = [
    ...edges.filter((edge) => edge.from === node.id).map((edge) => ({ arrow: "\u2192", kind: edge.kind, far: edge.to })),
    ...edges.filter((edge) => edge.to === node.id).map((edge) => ({ arrow: "\u2190", kind: edge.kind, far: edge.from })),
  ];
  if (links.length === 0) return null;

  return (
    <p className="mt-1 flex flex-wrap items-baseline gap-x-3 text-xs text-faint">
      {links.map((link, index) => (
        <span key={index}>
          {link.arrow} {link.kind} <span className="font-mono">{link.far}</span>
        </span>
      ))}
    </p>
  );
}
