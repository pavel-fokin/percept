import { useEffect, useState } from "react";

type Status = { events: number };

export default function App() {
  const [status, setStatus] = useState<Status>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    fetch("/api/status")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<Status>;
      })
      .then(setStatus)
      .catch((e: Error) => setError(e.message));
  }, []);

  return (
    <main className="p-8 font-sans">
      <h1 className="text-2xl font-semibold">percept</h1>
      {error ? (
        <p className="text-red-600">Failed to load status: {error}</p>
      ) : status ? (
        <p>{status.events} events</p>
      ) : (
        <p className="text-gray-500">Loading...</p>
      )}
    </main>
  );
}
