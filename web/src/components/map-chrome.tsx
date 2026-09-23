import type { Load, MapViewState } from "../hooks/use-map-view";
import NodeCard from "./node-card";
import Sheet from "./ui/sheet";

/** What a map page says while its map loads or fails - written once so
 * the outline and the board word it the same. */
export function MapStatus({ load, className }: { load: Load; className: string }) {
  return (
    <div aria-live="polite" className={`text-[0.8125rem] text-faint empty:hidden ${className}`}>
      {load.state === "loading" && <span>Reading the map&#8230;</span>}
      {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
    </div>
  );
}

/** The selected node's card: beside the page on a wide screen, in
 * `wideClassName`, and as the bottom sheet on a narrow one. */
export function SelectedCard({ view, wideClassName }: { view: MapViewState; wideClassName: string }) {
  const { map, outline, selected, setNode, close, wide } = view;
  if (!map || !outline || !selected) return null;
  const card = <NodeCard id={selected} edges={map.edges} outline={outline} onSelect={setNode} onClose={close} />;
  return wide ? (
    <div className={wideClassName}>{card}</div>
  ) : (
    <Sheet onClose={close} label={outline.byId.get(selected)?.name ?? ""}>
      {card}
    </Sheet>
  );
}
