import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type MapSummary, type Session } from "../api";
import { relative } from "../format";
import { textLink } from "../components/ui/text-link";
import { pathUrl } from "../paths";

const README = "https://github.com/pavel-fokin/percept#readme";

type PathGroup = { path: string; maps: MapSummary[]; sessions: number; latest?: Session };

/** One group per path, in order of each path's newest session; paths with only maps come last. */
export async function loadStart() {
  const [sessions, maps] = await Promise.all([
    getData<Session[]>("/api/sessions"),
    getData<MapSummary[]>("/api/maps"),
  ]);

  const groups = new Map<string, PathGroup>();
  const group = (path: string) => {
    if (!groups.has(path)) groups.set(path, { path, maps: [], sessions: 0 });
    return groups.get(path)!;
  };

  // Sessions come newest first, so a path's first session is its latest.
  for (const session of sessions) {
    const g = group(session.path);
    g.latest ??= session;
    g.sessions += 1;
  }
  for (const map of maps) group(map.path).maps.push(map);

  return [...groups.values()];
}

export function Start() {
  const groups = useLoaderData<typeof loadStart>();
  const now = new Date();

  if (groups.length === 0) return <EmptyState />;

  return (
    <div className="mt-8 grid gap-14 sm:mt-14">
      {groups.map(({ path, maps, sessions, latest }) => (
        <section key={path}>
          <div className="flex items-baseline justify-between gap-4">
            <h2 className="text-2xl font-semibold tracking-tight sm:text-[26px]">{name(path)}</h2>
            {latest && (
              <time dateTime={latest.created_at} className="flex-none text-sm text-muted tabular-nums">
                active {relative(new Date(latest.created_at), now)}
              </time>
            )}
          </div>
          <p className="mt-1.5 font-mono text-[13px] break-all text-muted">{path}</p>

          {latest && (
            <Link
              to={`/sessions/${latest.id}`}
              className="-mx-3 mt-6 flex min-h-11 items-center gap-3 rounded-[10px] px-3 py-3 hover:bg-surface focus-visible:bg-surface"
            >
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span className="text-[13px] text-muted">Latest session</span>
                <span className={`line-clamp-2 sm:line-clamp-1 ${latest.title ? "" : "font-mono text-sm text-muted"}`}>
                  {latest.title ?? latest.client_session_id}
                </span>
              </span>
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-4 flex-none text-muted">
                <path d="M6 3l5 5-5 5" />
              </svg>
            </Link>
          )}

          <nav aria-label={name(path)} className="-mx-3 mt-2 flex flex-wrap gap-1">
            {maps.map((map) => (
              <Counter key={map.id} to={`/maps/${map.id}`} label={map.kind} count={map.nodes} />
            ))}
            {sessions > 0 && <Counter to={pathUrl(path)} label="sessions" count={sessions} />}
          </nav>
        </section>
      ))}
    </div>
  );
}

/** The folder a path ends in: `~/code/percept` is `percept`. */
function name(path: string) {
  return path.split("/").filter(Boolean).at(-1) ?? path;
}

export function StartError() {
  return <p className="pt-6">Failed to load sessions: {describe(useRouteError())}</p>;
}

function Counter({ to, label, count }: { to: string; label: string; count: number }) {
  return (
    <Link to={to} className="group flex min-h-11 items-baseline gap-2 rounded-[10px] px-3 py-2.5 hover:bg-surface focus-visible:bg-surface">
      <span className="text-[17px] font-semibold tabular-nums">{count}</span>
      <span className="text-[15px] text-muted group-hover:text-text">{label}</span>
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
