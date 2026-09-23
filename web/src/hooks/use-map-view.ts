import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useSearchParams } from "react-router";
import { fetchMap, messageOf } from "../lib/api";
import { buildOutline } from "../lib/outline";
import type { MapResponse } from "../lib/types";

/** Where the selected node's card stops being a bottom sheet and sits
 * beside the map instead - the outline page and the board agree on
 * this width, so a reader sees the same layout switch on both. */
const WIDE = "(min-width: 900px)";

function subscribeWide(onChange: () => void) {
  const query = window.matchMedia(WIDE);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

export type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; map: MapResponse };

export type MapViewState = ReturnType<typeof useMapView>;

/** One cognitive map, loaded once and kept selected through the URL's
 * `node` param - the loading, selection and Escape handling the
 * outline page and the board both need, written once so neither
 * reinvents it. The page names the map, since each carries it in its
 * own place in the URL. */
export function useMapView(id: string) {
  const [params, setParams] = useSearchParams();
  const root = params.get("root") ?? "";
  const selected = params.get("node");
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const wide = useSyncExternalStore(subscribeWide, () => window.matchMedia(WIDE).matches);

  useEffect(() => {
    let cancelled = false;
    setLoad({ state: "loading" });
    fetchMap(id, root)
      .then((map) => {
        if (!cancelled) setLoad({ state: "ready", map });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoad({ state: "failed", message: messageOf(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [id, root]);

  const map = load.state === "ready" ? load.map : null;
  const outline = useMemo(() => (map ? buildOutline(map) : null), [map]);

  const setNode = useCallback(
    (nodeId: string | null) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (nodeId) next.set("node", nodeId);
          else next.delete("node");
          return next;
        },
        { replace: true },
      );
    },
    [setParams],
  );
  const close = useCallback(() => setNode(null), [setNode]);

  useEffect(() => {
    if (!selected || !wide) return;
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") close();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [selected, wide, close]);

  // An unknown `node` id shows no card - the map may have moved on
  // since the address was shared.
  const selectedNode = selected && outline?.byId.has(selected) ? selected : null;

  return { root, load, map, outline, selected: selectedNode, setNode, close, wide };
}
