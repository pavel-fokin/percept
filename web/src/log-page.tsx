import { useEffect, useRef, useState } from "react";
import { fetchEvents, messageOf } from "./api";
import { filterFromSearch, resolveSince, searchFromFilter } from "./filters";
import type { Filter } from "./filters";
import Header from "./header";
import Log from "./log";
import type { Event } from "./types";

interface Results {
  events: Event[];
  carried: Event[];
  total: number;
  query: string;
  updatedAt: number;
}

type Load =
  | { state: "loading"; results: null }
  | { state: "ready" | "updating"; results: Results }
  | { state: "failed"; results: Results | null; message: string };

const SEARCH_DEBOUNCE_MS = 250;

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading", results: null });
  const [project, setProject] = useState<string | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const [pagingFailed, setPagingFailed] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const [filter, setFilter] = useState<Filter>(() => filterFromSearch(window.location.search));

  const debouncedQ = useDebouncedValue(filter.q, SEARCH_DEBOUNCE_MS);
  const asked = searchFromFilter({ ...filter, q: debouncedQ });
  const currentQuery = useRef(asked);
  const pagingRequest = useRef(0);
  currentQuery.current = asked;

  useEffect(() => {
    window.history.replaceState(null, "", `${window.location.pathname}${asked}`);
  }, [asked]);

  useEffect(() => {
    const parsed = filterFromSearch(asked);
    const wanted = { ...parsed, since: resolveSince(parsed.since) };
    let cancelled = false;
    pagingRequest.current += 1;
    setLoadingMore(false);
    setPagingFailed(null);
    setLoad((held) => (held.results ? { state: "updating", results: held.results } : { state: "loading", results: null }));
    fetchEvents(wanted)
      .then((response) => {
        if (cancelled || currentQuery.current !== asked) return;
        setLoad({
          state: "ready",
          results: {
            events: response.events,
            carried: response.carried,
            total: response.total,
            query: asked,
            updatedAt: Date.now(),
          },
        });
        setProject(response.project);
      })
      .catch((error: unknown) => {
        if (cancelled || currentQuery.current !== asked) return;
        setLoad((held) => ({ state: "failed", results: held.results, message: messageOf(error) }));
      });
    return () => {
      cancelled = true;
    };
  }, [asked, retry]);

  function showEarlier() {
    const results = load.results;
    if (!results || load.state !== "ready" || results.query !== asked || filter.q !== debouncedQ || loadingMore) return;
    const oldest = results.events[0];
    if (!oldest) return;
    const requestedQuery = asked;
    const request = ++pagingRequest.current;
    const parsed = filterFromSearch(asked);
    setLoadingMore(true);
    setPagingFailed(null);
    fetchEvents({ ...parsed, since: resolveSince(parsed.since) }, oldest.created_at)
      .then((response) => {
        if (request !== pagingRequest.current || currentQuery.current !== requestedQuery) return;
        setLoad((held) => {
          if (!held.results || held.state !== "ready" || held.results.query !== requestedQuery) return held;
          return {
            state: "ready",
            results: {
              ...held.results,
              events: [...response.events, ...held.results.events],
              carried: [...response.carried, ...held.results.carried],
            },
          };
        });
      })
      .catch((error: unknown) => {
        if (request === pagingRequest.current && currentQuery.current === requestedQuery) {
          setPagingFailed(messageOf(error));
        }
      })
      .finally(() => {
        if (request === pagingRequest.current) setLoadingMore(false);
      });
  }

  const results = load.results;
  const visibleQuery = searchFromFilter(filter);
  const visibleLoadState = load.state === "ready" && results?.query !== visibleQuery ? "updating" : load.state;

  return (
    <div className="flex min-h-screen flex-col">
      <a
        href="#log"
        className="sr-only focus:not-sr-only focus:absolute focus:z-50 focus:bg-accent focus:p-4 focus:text-button"
      >
        Skip to the log
      </a>
      <Header />
      <Log
        project={project}
        events={results?.events ?? []}
        carried={results?.carried ?? []}
        total={results?.total ?? 0}
        resultsQuery={results?.query ?? null}
        updatedAt={results?.updatedAt ?? null}
        loadState={visibleLoadState}
        loadError={load.state === "failed" ? load.message : null}
        loadingMore={loadingMore}
        pagingFailed={pagingFailed}
        onRetry={() => setRetry((attempt) => attempt + 1)}
        onShowEarlier={showEarlier}
        filter={filter}
        onFilterChange={setFilter}
      />
    </div>
  );
}
