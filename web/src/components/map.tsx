import { useCallback, useEffect, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { fetchMap, messageOf } from "../lib/api";
import { basename } from "../lib/format";
import { buildOutline } from "../lib/outline";
import type { MapResponse } from "../lib/types";
import MapTree from "./map-tree";
import NodeCard from "./node-card";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; map: MapResponse };

/** One cognitive map, whole - rendered as an outline tree from the
 * nodes that head it, with the selected node's card beside it on a
 * wide screen. Selection lives in the URL's `node` param, so an
 * address can be shared straight at one node. */
export default function MapView() {
  const { id = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const root = params.get("root") ?? "";
  const selected = params.get("node");
  const [load, setLoad] = useState<Load>({ state: "loading" });

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
  const name = map?.map.name ?? "";
  const outline = useMemo(() => buildOutline(map?.nodes ?? [], map?.edges ?? []), [map]);

  const select = useCallback(
    (nodeId: string) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("node", nodeId);
          return next;
        },
        { replace: true },
      );
    },
    [setParams],
  );

  const close = useCallback(() => {
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("node");
        return next;
      },
      { replace: true },
    );
  }, [setParams]);

  useEffect(() => {
    if (!selected) return;
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") close();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [selected, close]);

  // An unknown `node` id shows no card - the map may have moved on
  // since the address was shared.
  const selectedNode = selected && map ? (map.nodes.some((node) => node.id === selected) ? selected : null) : null;

  return (
    <main
      id="map"
      className={
        "mx-auto w-full flex-1 px-4 pb-14 sm:px-8 " +
        (selectedNode
          ? "max-w-[62.5rem] min-[900px]:grid min-[900px]:grid-cols-[minmax(0,1fr)_360px] min-[900px]:items-start min-[900px]:gap-8"
          : "max-w-3xl")
      }
    >
      <div className="min-w-0">
        <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
          <h1 className="text-base font-medium tracking-tight text-ink">{name}</h1>
          {root && <p className="text-[0.8125rem] text-faint">&#183; from {basename(root)}</p>}
        </div>
        {map && <p className="mt-1 font-serif text-[1.0625rem] leading-relaxed text-muted">{map.map.purpose}</p>}

        <div aria-live="polite" className="mt-3 text-[0.8125rem] text-faint empty:hidden">
          {load.state === "loading" && <span>Reading the map&#8230;</span>}
          {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
        </div>

        {map && (
          <MapTree nodes={map.nodes} kinds={map.kinds} outline={outline} selected={selectedNode} onSelect={select} />
        )}
      </div>

      {map && selectedNode && (
        <div className="hidden min-[900px]:sticky min-[900px]:top-14 min-[900px]:mt-4 min-[900px]:block min-[900px]:max-h-[calc(100vh-4.5rem)] min-[900px]:overflow-auto min-[900px]:border-l min-[900px]:border-rule min-[900px]:pl-6 sm:min-[900px]:top-16">
          <NodeCard
            id={selectedNode}
            nodes={map.nodes}
            edges={map.edges}
            kinds={map.kinds}
            outline={outline}
            onSelect={select}
            onClose={close}
          />
        </div>
      )}
    </main>
  );
}
