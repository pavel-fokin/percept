import { useEffect, useState } from "react";
import { fetchEvents, messageOf } from "./api";
import Header from "./Header";
import Log from "./Log";
import type { EventsResponse } from "./types";

type Load = { state: "loading" } | { state: "failed"; message: string } | { state: "ready"; response: EventsResponse };

export default function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });

  useEffect(() => {
    fetchEvents()
      .then((response) => setLoad({ state: "ready", response }))
      .catch((error: unknown) => setLoad({ state: "failed", message: messageOf(error) }));
  }, []);

  return (
    <div className="flex min-h-screen flex-col">
      <Header />
      {load.state === "loading" && <p className="mx-auto w-full max-w-3xl px-4 pt-10 sm:px-8">Reading the log.</p>}
      {load.state === "failed" && (
        <p className="mx-auto w-full max-w-3xl px-4 pt-10 sm:px-8">The log could not be read: {load.message}. Reload to try again.</p>
      )}
      {load.state === "ready" && <Log events={load.response.events} total={load.response.total} />}
    </div>
  );
}
