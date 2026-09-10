import { useEffect, useState } from "react";
import Queue from "./Queue";
import type { ReviewResponse } from "./types";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; response: ReviewResponse };

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [current, setCurrent] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/review")
      .then((response) => {
        if (!response.ok) {
          throw new Error(`${response.status} ${response.statusText}`);
        }
        return response.json() as Promise<ReviewResponse>;
      })
      .then((response) => {
        setLoad({ state: "ready", response });
        setCurrent((current) => current ?? response.maps[0]?.name ?? null);
      })
      .catch((error: unknown) => {
        setLoad({ state: "failed", message: error instanceof Error ? error.message : String(error) });
      });
  }, []);

  return (
    <div className="min-h-screen pb-24 font-sans text-[15px] leading-normal">
      <header className="sticky top-0 z-10 border-b border-[var(--rule)] bg-[var(--ground)]">
        <div className="mx-auto flex max-w-2xl items-baseline gap-4 px-4 pt-3 pb-2.5">
          <h1 className="text-base font-bold">
            percept <span className="font-normal text-[var(--ink-2)]">review</span>
          </h1>
          {load.state === "ready" && (
            <nav className="ml-auto flex gap-4" aria-label="maps">
              {load.response.maps.map((map) => {
                const claimed = map.groups
                  .flatMap((group) => group.claims)
                  .filter((claim) => claim.standing === "claimed").length;
                return (
                  <button
                    key={map.name}
                    type="button"
                    aria-current={map.name === current ? "page" : "false"}
                    onClick={() => setCurrent(map.name)}
                    className={
                      "border-b-2 py-0.5 " +
                      (map.name === current
                        ? "border-[var(--ink)] text-[var(--ink)]"
                        : "border-transparent text-[var(--ink-2)]")
                    }
                  >
                    {map.name}
                    <b className="ml-1 font-normal text-[var(--ink-3)]">{claimed}</b>
                  </button>
                );
              })}
            </nav>
          )}
        </div>
      </header>
      <main className="mx-auto max-w-2xl px-4">
        {load.state === "loading" && <p className="py-8">Reading the log.</p>}
        {load.state === "failed" && (
          <p className="py-8">The queue could not be read: {load.message}. Reload to try again.</p>
        )}
        {load.state === "ready" &&
          (() => {
            const map = load.response.maps.find((map) => map.name === current);
            return map ? <Queue map={map} /> : null;
          })()}
      </main>
    </div>
  );
}
