import { useEffect, useState } from "react";
import { fetchEvents, messageOf } from "./api";
import { projectOf } from "./eventRows";
import Header from "./Header";
import Log from "./Log";
import type { Event } from "./types";

type Load = { state: "loading" } | { state: "failed"; message: string } | { state: "ready" };

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [events, setEvents] = useState<Event[]>([]);
  const [total, setTotal] = useState(0);
  const [loadingMore, setLoadingMore] = useState(false);

  useEffect(() => {
    fetchEvents()
      .then((response) => {
        setEvents(response.events);
        // The first answer is the only one that counts the whole log:
        // every later request is bounded by `until`, so its own total
        // counts what is left behind that bound, not what there is.
        setTotal(response.total);
        setLoad({ state: "ready" });
      })
      .catch((error: unknown) => setLoad({ state: "failed", message: messageOf(error) }));
  }, []);

  function showEarlier() {
    const oldest = events[0];
    if (!oldest || loadingMore) return;
    setLoadingMore(true);
    fetchEvents(oldest.created_at)
      .then((response) => setEvents((held) => [...response.events, ...held]))
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
        <Log events={events} total={total} loadingMore={loadingMore} onShowEarlier={showEarlier} />
      )}
    </div>
  );
}
