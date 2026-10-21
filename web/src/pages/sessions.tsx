import { Link } from "react-router";
import { getData, type Session } from "../api";
import { BackLink } from "../components/back-link";
import { RouteError } from "../components/route-error";
import { byDay, full, time, withChangedTimes } from "../format";
import { sessionUrl } from "../paths";

export async function loadSessions(project: string) {
  const sessions = (await getData<Session[]>("/api/sessions")).filter((s) => s.project === project);
  if (sessions.length === 0) throw new Response(null, { status: 404 });

  return { project, sessions };
}

export function Sessions({ project, sessions }: { project: string; sessions: Session[] }) {
  const days = byDay(sessions, (s) => s.created_at, new Date());

  return (
    <>
      <h1 className="mt-8 sm:mt-10">
        <BackLink to="/" project={project} collection="sessions">
          <span className="tabular-nums"> · {sessions.length}</span>
        </BackLink>
      </h1>

      <div className="mt-8 grid gap-8 sm:mt-10 sm:gap-10">
        {days.map((day) => (
          <section key={day.items[0].id}>
            <h2 className="mb-1 text-sm font-semibold text-muted">{day.name}</h2>
            <ul className="-mx-3 grid gap-0.5">
              {withChangedTimes(day.items, (s) => time.format(new Date(s.created_at))).map(({ item, shown }) => (
                <SessionRow key={item.id} session={item} shown={shown} />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </>
  );
}

function SessionRow({ session, shown }: { session: Session; shown?: string }) {
  const created = new Date(session.created_at);

  return (
    <li>
      <Link
        to={sessionUrl(session.project, session.id)}
        className="flex min-h-11 items-baseline gap-4 rounded-lg px-3 py-2.5 hover:bg-surface focus-visible:bg-surface sm:gap-6"
      >
        <span
          className={`line-clamp-2 min-w-0 flex-1 wrap-anywhere sm:line-clamp-1 ${
            session.title ? "" : "font-mono text-sm text-muted"
          }`}
        >
          {session.title ?? session.client_session_id}
        </span>
        {shown && (
          <time dateTime={session.created_at} title={full.format(created)} className="flex-none text-sm text-muted tabular-nums">
            {shown}
          </time>
        )}
      </Link>
    </li>
  );
}

export function SessionsError() {
  return <RouteError notFound="No sessions recorded in this project." failed="sessions" />;
}
