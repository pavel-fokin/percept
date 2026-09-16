import { useEffect, useState } from "react";
import { fetchEvents, messageOf } from "./api";
import { projectOf } from "./eventRows";
import { EMPTY_FILTER, filterFromSearch, searchFromFilter } from "./filters";
import type { Filter } from "./filters";
import Header from "./Header";
import Log from "./Log";
import type { Event } from "./types";

type Load = { state: "loading" } | { state: "failed"; message: string } | { state: "ready" };

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
  const [events, setEvents] = useState<Event[]>([]);
  const [carried, setCarried] = useState<Event[]>([]);
  const [total, setTotal] = useState(0);
  const [loadingMore, setLoadingMore] = useState(false);
  const [filter, setFilter] = useState<Filter>(() => filterFromSearch(window.location.search));

  // Only the search box is debounced (250ms); a kind, actor, or time
  // pick refetches at once, since none fires once per keystroke.
  const debouncedQ = useDebouncedValue(filter.q, SEARCH_DEBOUNCE_MS);
  const kindsKey = filter.kinds.join(",");
  const actorsKey = filter.actors.join(",");

  useEffect(() => {
    window.history.replaceState(null, "", `${window.location.pathname}${searchFromFilter(filter)}`);
  }, [filter]);

  useEffect(() => {
    let cancelled = false;
    fetchEvents({ q: debouncedQ, kinds: filter.kinds, actors: filter.actors, since: filter.since })
      .then((response) => {
        if (cancelled) return;
        setEvents(response.events);
        setCarried(response.carried);
        // The first answer for a filter is the only one that counts the
        // whole match: every later request is bounded by `until`, so
        // its own total counts what is left behind that bound, not
        // what there is.
        setTotal(response.total);
        setLoad({ state: "ready" });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoad({ state: "failed", message: messageOf(error) });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debouncedQ, kindsKey, actorsKey, filter.since]);

  function showEarlier() {
    const oldest = events[0];
    if (!oldest || loadingMore) return;
    setLoadingMore(true);
    fetchEvents({ q: debouncedQ, kinds: filter.kinds, actors: filter.actors, since: filter.since }, oldest.created_at)
      .then((response) => {
        setEvents((held) => [...response.events, ...held]);
        setCarried((held) => [...response.carried, ...held]);
      })
      .catch((error: unknown) => setLoad({ state: "failed", message: messageOf(error) }))
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
      <Header project={events.length > 0 ? projectOf(events[0]) : null} />
      <p role="status" className="mx-auto w-full max-w-3xl px-4 pt-10 sm:px-8 empty:hidden">
        {load.state === "loading" && "Reading the log."}
        {load.state === "failed" && `The log could not be read: ${load.message}. Reload to try again.`}
      </p>
      {load.state === "ready" && (
        <Log
          events={events}
          carried={carried}
          total={total}
          loadingMore={loadingMore}
          onShowEarlier={showEarlier}
          filter={filter}
          onFilterChange={setFilter}
          onClear={() => setFilter(EMPTY_FILTER)}
        />
      )}
    </div>
  );
}
