import { useEffect, useState } from "react";
import { confirm, dispute, fetchReview, finish } from "./api";
import { plural } from "./format";
import Queue from "./Queue";
import { FinishSheet, WhySheet } from "./Sheet";
import Toast from "./Toast";
import type { Claim, MapQueue, ReviewResponse } from "./types";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; response: ReviewResponse };

type Sheet = { kind: "why"; id: string } | { kind: "finish" } | null;

/** `id`'s claim or folded option, searched across every map - what the
 * why sheet shows a name beside, and what `w` resolves a focused row
 * to. */
function findClaim(response: ReviewResponse, id: string): Claim | { id: string; name: string } | undefined {
  for (const map of response.maps) {
    for (const group of map.groups) {
      for (const claim of group.claims) {
        if (claim.id === id) return claim;
        const option = claim.options.find((option) => option.id === id);
        if (option) return option;
      }
    }
  }
  return undefined;
}

/** The current map's headline claims still claimed - what Finish marks
 * seen, and what its sheet counts. */
function claimedRows(map: MapQueue): Claim[] {
  return map.groups.flatMap((group) => group.claims).filter((claim) => claim.standing === "claimed");
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
      .catch((error: unknown) => {
        setLoad({ state: "failed", message: error instanceof Error ? error.message : String(error) });
      });
  }

  useEffect(refetch, []);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  const map = load.state === "ready" ? load.response.maps.find((map) => map.name === current) : undefined;
  const rowIds = map ? map.groups.flatMap((group) => group.claims.map((claim) => claim.id)) : [];

  function moveFocus(delta: 1 | -1) {
    if (rowIds.length === 0) return;
    const at = focused ? rowIds.indexOf(focused) : -1;
    const next = at === -1 ? 0 : Math.min(Math.max(at + delta, 0), rowIds.length - 1);
    setFocused(rowIds[next]);
  }

  function openWhy(id: string) {
    setSheet({ kind: "why", id });
  }

  // Every keystroke already lands in `drafts` through the textarea's
  // `onChange`, so closing needs only to dismiss the sheet - what was
  // typed is kept until a later Save clears it.
  function closeWhy() {
    setSheet(null);
  }

  function saveWhy() {
    if (sheet?.kind !== "why" || load.state !== "ready" || !current) return;
    const { id } = sheet;
    const why = drafts[id] ?? "";
    dispute(current, id, why)
      .then(() => {
        setDrafts((drafts) => {
          const { [id]: _removed, ...rest } = drafts;
          return rest;
        });
        setSheet(null);
        refetch();
        toastNow(`Marked ${id} wrong.`);
      })
      .catch((error: unknown) => toastNow(error instanceof Error ? error.message : String(error)));
  }

  function confirmNode(id: string) {
    if (!current) return;
    confirm(current, id)
      .then(() => {
        refetch();
        toastNow(`Confirmed ${id}.`);
      })
      .catch((error: unknown) => toastNow(error instanceof Error ? error.message : String(error)));
  }

  function toastNow(text: string) {
    setToast(text);
  }

  function openFinish() {
    if (!map || claimedRows(map).length === 0) return;
    setSheet({ kind: "finish" });
  }

  function toggleSource(id: string) {
    const row = document.querySelector(`li[data-id="${CSS.escape(id)}"] details.source`);
    if (row instanceof HTMLDetailsElement) row.open = !row.open;
  }

  function markSeen() {
    if (!current || !map) return;
    const claimed = claimedRows(map);
    const count = claimed.length;
    const nodes = [...claimed.map((claim) => claim.id), ...map.groups.map((group) => group.id).filter(Boolean)];
    finish(current, nodes)
      .then(() => {
        setSheet(null);
        refetch();
        toastNow(`Review finished. ${plural(count, "One claim", "claims")} seen.`);
      })
      .catch((error: unknown) => toastNow(error instanceof Error ? error.message : String(error)));
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (sheet) return;
      const target = event.target as HTMLElement;
      if (target.matches("textarea, input, select")) return;
      if (event.key === "j") moveFocus(1);
      if (event.key === "k") moveFocus(-1);
      if (event.key === "?") setKeysShown((shown) => !shown);
      if (event.key === "f") openFinish();
      if (!focused) return;
      if (event.key === "w") openWhy(focused);
      if (event.key === "y") confirmNode(focused);
      if (event.key === "s") toggleSource(focused);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  });

  const whyClaim = sheet?.kind === "why" ? findClaim(load.state === "ready" ? load.response : { maps: [], next: null }, sheet.id) : undefined;

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
                const claimed = claimedRows(map).length;
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
        {load.state === "ready" && map && (
          <Queue map={map} focused={focused} onDispute={openWhy} onConfirm={confirmNode} />
        )}
        <section className="mt-12">
          <h2 className="text-base font-bold">What the next session starts with</h2>
          <p className="mt-1 text-[var(--ink-2)]">
            These lines are printed to the model when it next opens this project, in any client.
          </p>
          <pre className="mt-3 whitespace-pre-wrap break-words rounded-md bg-[var(--ground-2)] p-3.5 font-mono text-sm leading-relaxed text-[var(--ink)]">
            {load.state === "ready" && load.response.next
              ? load.response.next
              : "Nothing judged yet. Seen is not listed: it changes nothing the model does."}
          </pre>
        </section>
      </main>

      {keysShown && (
        <footer className="keys-only fixed inset-x-0 bottom-[3.9rem] z-[6] border-t border-[var(--rule)] bg-[var(--sheet)]">
          <div className="mx-auto grid max-w-2xl grid-cols-[repeat(auto-fit,minmax(9rem,1fr))] gap-x-4 gap-y-1.5 px-4 py-3 text-[var(--ink-2)]">
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">j</kbd>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">k</kbd>
              next, previous
            </span>
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">w</kbd>
              wrong
            </span>
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">y</kbd>
              confirm
            </span>
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">s</kbd>
              show the exchange
            </span>
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">f</kbd>
              finish, after a check
            </span>
            <span>
              <kbd className="mr-1 rounded border border-[var(--rule)] px-1.5 font-bold text-[var(--ink)]">?</kbd>
              hide this
            </span>
          </div>
        </footer>
      )}
      <div className="fixed inset-x-0 bottom-0 z-[6] border-t border-[var(--rule)] bg-[var(--ground)] pb-[env(safe-area-inset-bottom)]">
        <div className="mx-auto flex max-w-2xl items-center gap-3 px-4 py-2.5">
          <button
            type="button"
            disabled={!map || claimedRows(map).length === 0}
            onClick={openFinish}
            className="min-h-11 flex-1 rounded-md bg-[var(--ink)] px-4 py-2 font-bold text-[var(--ground)] disabled:bg-[var(--ground-2)] disabled:text-[var(--ink-3)]"
          >
            {map && claimedRows(map).length === 0 ? "Review finished" : "Finish review"}
          </button>
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
          name={whyClaim?.name ?? ""}
          value={drafts[sheet.id] ?? ""}
          onChange={(value) => setDrafts((drafts) => ({ ...drafts, [sheet.id]: value }))}
          onCancel={closeWhy}
          onSave={saveWhy}
        />
      )}
      {sheet?.kind === "finish" && map && (
        <FinishSheet
          map={map.name}
          count={claimedRows(map).length}
          onCancel={() => setSheet(null)}
          onConfirm={markSeen}
        />
      )}
      <Toast text={toast} />
    </div>
  );
}
