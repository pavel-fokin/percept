import { useEffect, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { fetchMap, messageOf } from "../lib/api";
import { basename } from "../lib/format";
import type { MapResponse } from "../lib/types";
import MapTree from "./map-tree";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; map: MapResponse };

/** One cognitive map, whole - rendered as an outline tree from the
 * nodes that head it. */
export default function MapView() {
  const { id = "" } = useParams();
  const [params] = useSearchParams();
  const root = params.get("root") ?? "";
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

  return (
    <main id="map" className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
        <h1 className="text-base font-medium tracking-tight text-ink">{name}</h1>
        {root && <p className="text-[0.8125rem] text-faint">&#183; from {basename(root)}</p>}
      </div>
      {map && <p className="mt-1 font-serif text-[1.0625rem] leading-relaxed text-muted">{map.map.purpose}</p>}

      <div aria-live="polite" className="mt-3 text-[0.8125rem] text-faint empty:hidden">
        {load.state === "loading" && <span>Reading the map&#8230;</span>}
        {load.state === "failed" && <span className="text-ink">This map could not be read: {load.message}.</span>}
      </div>

      {map && <MapTree nodes={map.nodes} edges={map.edges} kinds={map.kinds} />}
    </main>
  );
}
