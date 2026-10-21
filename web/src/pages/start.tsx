import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type MapSummary, type Session } from "../api";
import { relative } from "../format";
import { Spinner } from "../components/spinner";
import { textLink } from "../components/ui/text-link";
import { mapUrl, sessionsUrl, sessionUrl } from "../paths";
import { usePolling } from "../use-polling";

const README = "https://github.com/pavel-fokin/percept#readme";

type ProjectGroup = { project: string; maps: MapSummary[]; sessions: number; latest?: Session };

/** One group per project, in order of each project's newest session; projects with only maps come last. */
export async function loadStart() {
  const [sessions, maps] = await Promise.all([
    getData<Session[]>("/api/sessions"),
    getData<MapSummary[]>("/api/maps"),
  ]);

  const groups = new Map<string, ProjectGroup>();
  const group = (project: string) => {
    if (!groups.has(project)) groups.set(project, { project, maps: [], sessions: 0 });
    return groups.get(project)!;
  };

  // Latest by activity, so a working session stays in view when a newer one sits idle beside it.
  for (const session of sessions) {
    const g = group(session.project);
    if (!g.latest || new Date(session.last_event_at) > new Date(g.latest.last_event_at)) g.latest = session;
    g.sessions += 1;
  }
  for (const map of maps) group(map.project).maps.push(map);

  return [...groups.values()];
}

export function Start() {
  const groups = useLoaderData<typeof loadStart>();
  usePolling(true);
  const now = new Date();

  if (groups.length === 0) return <EmptyState />;

  return (
    <div className="mt-8 grid gap-14 sm:mt-14">
      {groups.map(({ project, maps, sessions, latest }) => (
        <section key={project}>
          <div className="flex items-baseline justify-between gap-4">
            <h2 className="min-w-0 font-mono text-sm font-normal wrap-anywhere">{project}</h2>
            {latest && (
              <time dateTime={latest.last_event_at} className="flex-none text-sm text-muted tabular-nums">
                active {relative(new Date(latest.last_event_at), now)}
              </time>
            )}
          </div>

          {latest && (
            <Link
              to={sessionUrl(project, latest.id)}
              className="-mx-3 mt-6 flex min-h-11 items-center gap-3 rounded-lg px-3 py-3 hover:bg-surface focus-visible:bg-surface"
            >
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span className="flex items-center gap-1.5 text-sm text-muted">
                  Latest session
                  {latest.status === "working" && <Working />}
                </span>
                <span className={`line-clamp-2 sm:line-clamp-1 ${latest.title ? "" : "font-mono text-sm text-muted"}`}>
                  {latest.title ?? latest.client_session_id}
                </span>
              </span>
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-4 flex-none text-muted">
                <path d="M6 3l5 5-5 5" />
              </svg>
            </Link>
          )}

          <nav aria-label={project} className="-mx-3 mt-2 flex flex-wrap gap-1">
            {maps.map((map) => (
              <Counter key={map.id} to={mapUrl(project, map.kind)} label={map.kind} count={map.nodes} />
            ))}
            {sessions > 0 && <Counter to={sessionsUrl(project)} label="sessions" count={sessions} />}
          </nav>
        </section>
      ))}
    </div>
  );
}

export function StartError() {
  return <p className="pt-6">Failed to load sessions: {describe(useRouteError())}</p>;
}

// Only working is marked: a client that never sends SessionEnd leaves its session waiting for good.
function Working() {
  return (
    <>
      <span aria-hidden>·</span>
      <Spinner />
      working
    </>
  );
}

function Counter({ to, label, count }: { to: string; label: string; count: number }) {
  return (
    <Link to={to} className="group flex min-h-11 items-baseline gap-2 rounded-lg px-3 py-2.5 hover:bg-surface focus-visible:bg-surface">
      <span className="text-lg font-semibold tabular-nums">{count}</span>
      <span className="text-base text-muted group-hover:text-text">{label}</span>
    </Link>
  );
}

function EmptyState() {
  return (
    <section className="grid justify-items-center gap-3 py-16 text-center">
      <h2 className="text-2xl font-semibold text-balance">No sessions yet</h2>
      <p className="max-w-[36ch] text-muted">
        Sessions appear here once a coding client sends its first hook event.
      </p>
      <a href={README} className={`mt-2 ${textLink}`}>
        Set up a client
      </a>
    </section>
  );
}
