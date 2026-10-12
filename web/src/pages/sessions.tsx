import { useLoaderData, useRouteError } from "react-router";
import { describe, getData, type Session } from "../api";
import { SessionRow } from "../components/session-row";

const README = "https://github.com/pavel-fokin/percept#readme";

export function loadSessions() {
  return getData<Session[]>("/api/sessions");
}

export function Sessions() {
  const sessions = useLoaderData<typeof loadSessions>();

  if (sessions.length === 0) return <EmptyState />;

  const now = new Date();

  return (
    <>
      <h2 className="pt-6 pb-2 text-sm font-semibold text-muted">Sessions</h2>
      <ul>
        {sessions.map((session) => (
          <SessionRow key={session.id} session={session} now={now} />
        ))}
      </ul>
    </>
  );
}

export function SessionsError() {
  return <p className="pt-6">Failed to load sessions: {describe(useRouteError())}</p>;
}

function EmptyState() {
  return (
    <section className="grid justify-items-center gap-3 py-16 text-center">
      <h2 className="text-2xl font-semibold text-balance">No sessions yet</h2>
      <p className="max-w-[36ch] text-muted">
        Sessions appear here once a coding client sends its first hook event.
      </p>
      <a href={README} className="mt-2 underline decoration-muted underline-offset-3 hover:decoration-text">
        Set up a client
      </a>
    </section>
  );
}
