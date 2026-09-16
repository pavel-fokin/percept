/** The two pages the app has, and the one place a path becomes one.
 * Paths rather than hashes: the log's filters already live in the query
 * string, so `/log?q=drift` is a link the server and the page read the
 * same way, where a hash would have to carry a query string of its
 * own. */

import { useSyncExternalStore } from "react";

export type Route = "index" | "log";

export const PATHS: Record<Route, string> = {
  index: "/",
  log: "/log",
};

/** The route a path names. Anything unrecognised is the index: the
 * server serves this page on every path it does not own, so a typed or
 * stale URL lands somewhere real rather than on a blank screen. */
export function routeOf(pathname: string): Route {
  return pathname.replace(/\/+$/, "") === PATHS.log ? "log" : "index";
}

const listeners = new Set<() => void>();

/** Moves to `to` - a path, with its query string when it has one - and
 * tells every mounted `useRoute`. `pushState` raises no event of its
 * own, which is why the subscriber set exists. */
export function navigate(to: string): void {
  if (to === `${window.location.pathname}${window.location.search}`) return;
  window.history.pushState(null, "", to);
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  window.addEventListener("popstate", listener);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("popstate", listener);
  };
}

/** The route on screen, following both `navigate` and the browser's
 * own back and forward. */
export function useRoute(): Route {
  return useSyncExternalStore(subscribe, () => routeOf(window.location.pathname));
}

/** The log's search field, named once: the field wears it as its `id`,
 * the label points at it, and a link that means "search the log"
 * carries it as a fragment. */
export const SEARCH_FIELD_ID = "q";
export const SEARCH_HASH = `#${SEARCH_FIELD_ID}`;
