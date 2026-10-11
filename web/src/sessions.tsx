import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type Session } from "./api";
import { full, relative } from "./format";

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

function SessionRow({ session, now }: { session: Session; now: Date }) {
  const started = new Date(session.created_at);

  return (
    <li className="border-t border-border first:border-t-0 has-[>a:hover]:border-transparent [li:has(>a:hover)+&]:border-transparent">
      <Link
        to={`/sessions/${session.id}`}
        className="-mx-3 flex min-h-11 flex-col gap-0.5 rounded-md p-3 hover:bg-surface focus-visible:bg-surface md:flex-row md:items-baseline md:gap-6"
      >
        <span
          className={`min-w-0 flex-1 line-clamp-2 text-base md:line-clamp-1 md:text-sm ${
            session.title ? "" : "font-mono text-muted"
          }`}
        >
          {session.title ?? session.key}
        </span>
        <time
          dateTime={session.created_at}
          title={full.format(started)}
          className="flex-none text-xs whitespace-nowrap text-muted tabular-nums"
        >
          {relative(started, now)}
        </time>
      </Link>
    </li>
  );
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
