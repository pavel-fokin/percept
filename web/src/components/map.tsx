import { Link, useParams } from "react-router";
import { useMapView } from "../hooks/use-map-view";
import { basename } from "../lib/format";
import { boardPath } from "../lib/routes";
import { MapStatus, SelectedCard } from "./map-chrome";
import MapTree from "./map-tree";

/** One cognitive map, whole - rendered as an outline tree from the
 * nodes that head it, with the selected node's card beside it on a
 * wide screen. Selection lives in the URL's `node` param, so an
 * address can be shared straight at one node. */
export default function MapView() {
  const { id = "" } = useParams();
  const view = useMapView(id);
  const { root, load, map, outline, selected, setNode, wide } = view;

  return (
    <main
      id="map"
      className={
        "mx-auto w-full flex-1 px-4 pb-14 sm:px-8 " +
        (selected
          ? "max-w-[62.5rem]" + (wide ? " grid grid-cols-[minmax(0,1fr)_360px] items-start gap-8" : "")
          : "max-w-3xl")
      }
    >
      <div className="min-w-0">
        <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
          <h1 className="text-base font-medium tracking-tight text-ink">{map?.map.name ?? ""}</h1>
          {root && <p className="text-[0.8125rem] text-faint">&#183; from {basename(root)}</p>}
          {map && (
            <Link
              to={boardPath(id, root)}
              className="text-[0.8125rem] text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
            >
              Open on a board
            </Link>
          )}
        </div>
        {map && <p className="mt-1 font-serif text-[1.0625rem] leading-relaxed text-muted">{map.map.purpose}</p>}

        <MapStatus load={load} className="mt-3" />

        {map && outline && (
          <MapTree key={map.map.id} kinds={map.kinds} outline={outline} selected={selected} onSelect={setNode} />
        )}
      </div>

      <SelectedCard
        view={view}
        wideClassName="sticky top-16 mt-4 max-h-[calc(100vh-4.5rem)] overflow-auto border-l border-rule pl-6"
      />
    </main>
  );
}
