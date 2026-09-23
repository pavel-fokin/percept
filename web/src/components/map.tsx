import { basename } from "../lib/format";
import { boardPath } from "../lib/routes";
import { useMapView } from "../lib/use-map-view";
import { Link } from "react-router";
import MapTree from "./map-tree";
import NodeCard from "./node-card";
import Sheet from "./ui/sheet";

/** One cognitive map, whole - rendered as an outline tree from the
 * nodes that head it, with the selected node's card beside it on a
 * wide screen. Selection lives in the URL's `node` param, so an
 * address can be shared straight at one node. */
export default function MapView() {
  const { id, root, load, map, outline, selected: selectedNode, setNode, close, wide } = useMapView();

  const name = map?.map.name ?? "";

  const card = map && outline && selectedNode && (
    <NodeCard id={selectedNode} edges={map.edges} outline={outline} onSelect={setNode} onClose={close} />
  );

  return (
    <main
      id="map"
      className={
        "mx-auto w-full flex-1 px-4 pb-14 sm:px-8 " +
        (selectedNode
          ? "max-w-[62.5rem]" + (wide && card ? " grid grid-cols-[minmax(0,1fr)_360px] items-start gap-8" : "")
          : "max-w-3xl")
      }
    >
      <div className="min-w-0">
        <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
          <h1 className="text-base font-medium tracking-tight text-ink">{name}</h1>
          {root && <p className="text-[0.8125rem] text-faint">&#183; from {basename(root)}</p>}
          {map && (
            <Link to={boardPath(id, root)} className="text-[0.8125rem] text-accent hover:underline">
              Open on a board
            </Link>
          )}
        </div>
        {map && <p className="mt-1 font-serif text-[1.0625rem] leading-relaxed text-muted">{map.map.purpose}</p>}

        <div aria-live="polite" className="mt-3 text-[0.8125rem] text-faint empty:hidden">
          {load.state === "loading" && <span>Reading the map&#8230;</span>}
          {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
        </div>

        {map && outline && (
          <MapTree key={map.map.id} kinds={map.kinds} outline={outline} selected={selectedNode} onSelect={setNode} />
        )}
      </div>

      {card &&
        (wide ? (
          <div className="sticky top-16 mt-4 max-h-[calc(100vh-4.5rem)] overflow-auto border-l border-rule pl-6">{card}</div>
        ) : (
          <Sheet onClose={close} label={outline?.byId.get(selectedNode)?.name ?? ""}>
            {card}
          </Sheet>
        ))}
    </main>
  );
}
