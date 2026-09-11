import { useCallback, useEffect, useState } from "react";
import { change, fetchReview } from "./api";
import { messageOf } from "./format";
import Queue from "./Queue";
import { allClaims } from "./claims";
import { WhySheet } from "./Sheet";
import Toast from "./Toast";
import type { ReviewResponse } from "./types";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; response: ReviewResponse };

type Sheet = { kind: "why"; id: string; name: string } | null;

/** One keyboard shortcut's key cap - what the identical `<kbd>`
 * elements in the shortcuts footer shared inline. */
function Key({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">{children}</kbd>
  );
}

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [current, setCurrent] = useState<string | null>(null);
  const [focused, setFocused] = useState<string | null>(null);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [toast, setToast] = useState<string | null>(null);
  const [keysShown, setKeysShown] = useState(false);

  function refetch() {
    fetchReview()
      .then((response) => {
        setLoad({ state: "ready", response });
        setCurrent((current) => current ?? response.maps[0]?.name ?? null);
      })
      .catch((error: unknown) => setLoad({ state: "failed", message: messageOf(error) }));
  }

  useEffect(refetch, []);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  const map = load.state === "ready" ? load.response.maps.find((map) => map.name === current) : undefined;
  const rowIds = map ? allClaims(map).map((claim) => claim.id) : [];

  function moveFocus(delta: 1 | -1) {
    if (rowIds.length === 0) return;
    const at = focused ? rowIds.indexOf(focused) : -1;
    const next = at === -1 ? 0 : Math.min(Math.max(at + delta, 0), rowIds.length - 1);
    setFocused(rowIds[next]);
  }

  const onWrong = useCallback(
    (id: string) => {
      const name = map ? allClaims(map).find((claim) => claim.id === id)?.name : undefined;
      setSheet({ kind: "why", id, name: name ?? "" });
    },
    [map],
  );

  function saveWhy(id: string, why: string) {
    if (!current) return;
    change(current, id, why)
      .then(() => {
        setDrafts((drafts) => {
          const { [id]: _removed, ...rest } = drafts;
          return rest;
        });
        setSheet(null);
        refetch();
        setToast(`Marked ${id} wrong.`);
      })
      .catch((error: unknown) => setToast(messageOf(error)));
  }

  function toggleSource(id: string) {
    const row = document.querySelector(`li[data-id="${CSS.escape(id)}"] details.source`);
    if (row instanceof HTMLDetailsElement) row.open = !row.open;
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (sheet) return;
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target as HTMLElement;
      if (target.matches("textarea, input, select")) return;
      if (event.key === "j") {
        event.preventDefault();
        moveFocus(1);
      }
      if (event.key === "k") {
        event.preventDefault();
        moveFocus(-1);
      }
      if (event.key === "?") {
        event.preventDefault();
        setKeysShown((shown) => !shown);
      }
      if (!focused) return;
      if (event.key === "w") {
        event.preventDefault();
        onWrong(focused);
      }
      if (event.key === "s") {
        event.preventDefault();
        toggleSource(focused);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [sheet, focused, rowIds, map, onWrong]);

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
                const count = allClaims(map).length;
                return (
                  <button
                    key={map.name}
                    type="button"
                    aria-current={map.name === current ? "page" : "false"}
                    onClick={() => {
                      setCurrent(map.name);
                      setFocused(null);
                    }}
                    className={
                      "border-b-2 py-0.5 " +
                      (map.name === current
                        ? "border-[var(--ink)] text-[var(--ink)]"
                        : "border-transparent text-[var(--ink-2)]")
                    }
                  >
                    {map.name}
                    <b className="ml-1 font-normal text-[var(--ink-3)]">{count}</b>
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
        {load.state === "ready" && map && <Queue map={map} focused={focused} onDispute={onWrong} />}
      </main>

      {keysShown && (
        <footer className="keys-only fixed inset-x-0 bottom-[3.9rem] z-[6] border-t border-[var(--rule)] bg-[var(--sheet)]">
          <div className="mx-auto grid max-w-2xl grid-cols-[repeat(auto-fit,minmax(9rem,1fr))] gap-x-4 gap-y-1.5 px-4 py-3 text-[var(--ink-2)]">
            <span>
              <Key>j</Key>
              <Key>k</Key>
              next, previous
            </span>
            <span>
              <Key>w</Key>
              wrong
            </span>
            <span>
              <Key>s</Key>
              show the exchange
            </span>
            <span>
              <Key>?</Key>
              hide this
            </span>
          </div>
        </footer>
      )}
      <div className="fixed inset-x-0 bottom-0 z-[6] border-t border-[var(--rule)] bg-[var(--ground)] pb-[env(safe-area-inset-bottom)]">
        <div className="mx-auto flex max-w-2xl items-center justify-end gap-3 px-4 py-2.5">
          <button
            type="button"
            aria-label="keyboard shortcuts"
            onClick={() => setKeysShown((shown) => !shown)}
            className="keys-only min-h-11 w-11 rounded-md border-[1.5px] border-[var(--rule)] text-[var(--ink-2)]"
          >
            ?
          </button>
        </div>
      </div>

      {sheet?.kind === "why" && (
        <WhySheet
          id={sheet.id}
          name={sheet.name}
          initial={drafts[sheet.id] ?? ""}
          onCancel={(value) => {
            setDrafts((drafts) => ({ ...drafts, [sheet.id]: value }));
            setSheet(null);
          }}
          onSave={(value) => saveWhy(sheet.id, value)}
        />
      )}
      <Toast text={toast} />
    </div>
  );
}
