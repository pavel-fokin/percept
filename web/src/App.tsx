import { useEffect, useState } from "react";
import { fetchEvents, messageOf } from "./api";
import { basename } from "./eventRows";
import { filterFromSearch, resolveSince, searchFromFilter } from "./filters";
import type { Filter } from "./filters";
import Header from "./Header";
import Log from "./Log";
import type { Event } from "./types";

/** What the page has. The events, what they carry and how many matched
 * are only ever written together, so they live in the arm that has
 * them rather than beside it - "ready with nothing loaded" is then not
 * a state anything has to guard against. */
type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; events: Event[]; carried: Event[]; total: number };

const SEARCH_DEBOUNCE_MS = 250;

/** `value`, `delay` ms after it stops changing - the only debounce this
 * app needs, so it isn't worth a dependency. */
function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [project, setProject] = useState<string | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const [pagingFailed, setPagingFailed] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>(() => filterFromSearch(window.location.search));

  // Only the search box is debounced; a kind, actor, or time pick
  // refetches at once, since none of them fires once per keystroke.
  const debouncedQ = useDebouncedValue(filter.q, SEARCH_DEBOUNCE_MS);
  // The identity of the request, as a string: React compares the
  // dependency list by value, and `kinds` and `actors` are new arrays
  // on every keystroke. It doubles as the guard a slow page's answer
  // is checked against, so an answer for a filter nobody is looking at
  // any more is dropped rather than prepended.
  const asked = searchFromFilter({ ...filter, q: debouncedQ });

  useEffect(() => {
    window.history.replaceState(null, "", `${window.location.pathname}${asked}`);
  }, [asked]);

  useEffect(() => {
    const wanted = { ...filterFromSearch(asked), since: resolveSince(filterFromSearch(asked).since) };
    let cancelled = false;
    setPagingFailed(null);
    fetchEvents(wanted)
      .then((response) => {
        if (cancelled) return;
        setLoad({ state: "ready", events: response.events, carried: response.carried, total: response.total });
        setProject(basename(response.project));
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoad({ state: "failed", message: messageOf(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [asked]);

  function showEarlier() {
    if (load.state !== "ready" || loadingMore) return;
    const oldest = load.events[0];
    if (!oldest) return;
    const requested = asked;
    setLoadingMore(true);
    setPagingFailed(null);
    fetchEvents({ ...filterFromSearch(asked), since: resolveSince(filterFromSearch(asked).since) }, oldest.created_at)
      .then((response) => {
        // A filter changed while this was in flight, so its rows belong
        // to a page nobody is reading - prepending them would put rows
        // the filter excludes at the top of the list.
        if (requested !== asked) return;
        setLoad((held) =>
          held.state === "ready"
            ? { ...held, events: [...response.events, ...held.events], carried: [...response.carried, ...held.carried] }
            : held,
        );
      })
      // An earlier page failing says nothing about the rows already
      // read, so the page keeps them and says so on the line the
      // request came from.
      .catch((error: unknown) => setPagingFailed(messageOf(error)))
      .finally(() => setLoadingMore(false));
  }

  return (
    <div className="flex min-h-screen flex-col">
      <a
        href="#log"
        className="sr-only focus:not-sr-only focus:absolute focus:z-50 focus:bg-accent focus:p-4 focus:text-button"
      >
        Skip to the log
      </a>
      <Header project={project} />
      <p role="status" className="mx-auto w-full max-w-3xl px-4 pt-10 sm:px-8 empty:hidden">
        {load.state === "loading" && "Reading the log."}
        {load.state === "failed" && `The log could not be read: ${load.message}. Reload to try again.`}
      </p>
      {load.state === "ready" && (
        <Log
          events={load.events}
          carried={load.carried}
          total={load.total}
          loadingMore={loadingMore}
          pagingFailed={pagingFailed}
          onShowEarlier={showEarlier}
          filter={filter}
          onFilterChange={setFilter}
        />
      )}
    </div>
  );
}
