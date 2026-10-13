import { Link, useLoaderData, type LoaderFunctionArgs } from "react-router";
import { getData, type Session } from "../api";
import { RouteError } from "../components/route-error";
import { dayName, full, time } from "../format";
import { pathName, urlPath } from "../paths";

export async function loadPathSessions({ params }: LoaderFunctionArgs) {
  const path = urlPath(params["*"] ?? "");
  if (path === null) throw new Response(null, { status: 404 });

  const sessions = (await getData<Session[]>("/api/sessions")).filter((s) => s.path === path);
  if (sessions.length === 0) throw new Response(null, { status: 404 });

  return { path, sessions };
}

/** Sessions come newest first, so consecutive runs of one day are its group. */
function byDay(sessions: Session[], now: Date) {
  const days: { name: string; sessions: Session[] }[] = [];

  for (const session of sessions) {
    const name = dayName(new Date(session.created_at), now);
    const last = days.at(-1);
    if (last?.name === name) last.sessions.push(session);
    else days.push({ name, sessions: [session] });
  }

  return days;
}

export function PathSessions() {
  const { path, sessions } = useLoaderData<typeof loadPathSessions>();
  const days = byDay(sessions, new Date());

  return (
    <>
      <div className="mt-8 sm:mt-10">
        <p className="text-sm text-muted">{pathName(path)}</p>
        <h1 className="mt-1.5 flex items-baseline gap-2.5 text-2xl font-semibold tracking-tight sm:text-[26px]">
          Sessions
          <span className="text-[17px] font-normal tracking-normal text-muted tabular-nums">{sessions.length}</span>
        </h1>
      </div>

      <div className="mt-8 grid gap-8 sm:mt-10 sm:gap-10">
        {days.map((day) => (
          <section key={day.name}>
            <h2 className="mb-1 text-sm font-semibold text-muted">{day.name}</h2>
            <ul className="-mx-3 grid gap-0.5">
              {day.sessions.map((session) => (
                <SessionRow key={session.id} session={session} />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </>
  );
}

function SessionRow({ session }: { session: Session }) {
  const created = new Date(session.created_at);

  return (
    <li>
      <Link
        to={`/sessions/${session.id}`}
        className="flex min-h-11 items-baseline gap-4 rounded-[10px] px-3 py-2.5 hover:bg-surface focus-visible:bg-surface sm:gap-6"
      >
        <span
          className={`line-clamp-2 min-w-0 flex-1 wrap-anywhere sm:line-clamp-1 ${
            session.title ? "" : "font-mono text-sm text-muted"
          }`}
        >
          {session.title ?? session.client_session_id}
        </span>
        <time dateTime={session.created_at} title={full.format(created)} className="flex-none text-[13px] text-muted tabular-nums">
          {time.format(created)}
        </time>
      </Link>
    </li>
  );
}

export function PathSessionsError() {
  return <RouteError notFound="No sessions recorded at this path." failed="sessions" />;
}
