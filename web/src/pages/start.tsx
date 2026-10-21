import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type BoardSummary, type MapSummary, type Project, type Session } from "../api";
import { relative } from "../format";
import { NewBoard } from "../components/new-board";
import { Spinner } from "../components/spinner";
import { chip } from "../components/ui/chip";
import { textLink } from "../components/ui/text-link";
import { boardUrl, mapUrl, sessionsUrl, sessionUrl } from "../paths";
import { usePolling } from "../use-polling";

const README = "https://github.com/pavel-fokin/percept#readme";

type ProjectGroup = { project: string; root: string; maps: MapSummary[]; boards: BoardSummary[]; sessions: number; latest?: Session };

const hasContent = (g: Pick<ProjectGroup, "maps" | "boards">) => g.maps.length > 0 || g.boards.length > 0;

/** One group per project: those with sessions by newest session, then those with only maps or boards, then empty ones. */
export async function loadStart() {
  const [projects, sessions, maps, boards] = await Promise.all([
    getData<Project[]>("/api/projects"),
    getData<Session[]>("/api/sessions"),
    getData<MapSummary[]>("/api/maps"),
    getData<BoardSummary[]>("/api/boards"),
  ]);

  const groups = new Map<string, ProjectGroup>(
    projects.map(({ name, root }) => [name, { project: name, root, maps: [], boards: [], sessions: 0 }]),
  );

  // Latest by activity, so a working session stays in view when a newer one sits idle beside it.
  const active = new Set<ProjectGroup>();
  for (const session of sessions) {
    const g = groups.get(session.project)!;
    if (!g.latest || new Date(session.last_event_at) > new Date(g.latest.last_event_at)) g.latest = session;
    g.sessions += 1;
    active.add(g);
  }
  for (const map of maps) groups.get(map.project)!.maps.push(map);
  for (const board of boards) groups.get(board.project)!.boards.push(board);

  const rest = [...groups.values()].filter((g) => !active.has(g));
  return [...active, ...rest.filter(hasContent), ...rest.filter((g) => !hasContent(g))];
}

export function Start() {
  const groups = useLoaderData<typeof loadStart>();
  usePolling(true);
  const now = new Date();

  if (groups.length === 0) return <EmptyState />;

  return (
    <div className="mt-8 grid gap-14 sm:mt-14">
      {groups.map(({ project, root, maps, boards, sessions, latest }) => {
        const isEmpty = sessions === 0 && !hasContent({ maps, boards });

        return (
          <section key={project}>
            <div className="flex items-baseline justify-between gap-4">
              <h2 className="min-w-0 font-mono text-sm font-normal wrap-anywhere">{project}</h2>
              {latest && (
                <time dateTime={latest.last_event_at} className="flex-none text-sm text-muted tabular-nums">
                  active {relative(new Date(latest.last_event_at), now)}
                </time>
              )}
              {isEmpty && <span className="flex-1 text-right font-mono text-sm text-muted wrap-anywhere">{root}</span>}
            </div>

            {isEmpty && (
              <p className="mt-6 text-muted">
                No sessions yet. Start a coding session in this folder.{" "}
                <a href={README} className={textLink}>
                  Set up a client
                </a>
              </p>
            )}

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

            {(maps.length > 0 || sessions > 0) && (
              <nav aria-label={project} className="-mx-3 mt-2 flex flex-wrap gap-1">
                {maps.map((map) => (
                  <Counter key={map.kind} to={mapUrl(project, map.kind)} label={map.kind} count={map.nodes} />
                ))}
                {sessions > 0 && <Counter to={sessionsUrl(project)} label="sessions" count={sessions} />}
              </nav>
            )}

            <nav aria-label={`${project} boards`} className="-mx-3 mt-1 flex flex-wrap items-center gap-1">
              {boards.map((board) => (
                <Link
                  key={board.id}
                  to={boardUrl(project, board.name)}
                  className={`group flex items-center gap-2 ${chip}`}
                >
                  <BoardIcon />
                  <span className="text-base text-muted group-hover:text-text">{board.name}</span>
                </Link>
              ))}
              <NewBoard project={project} />
            </nav>
          </section>
        );
      })}
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

function BoardIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="size-4 flex-none text-muted">
      <rect x="1.75" y="1.75" width="6" height="4.5" rx="1" />
      <rect x="8.25" y="9.75" width="6" height="4.5" rx="1" />
      <path d="M4.75 6.25v3.75a2 2 0 0 0 2 2h1.5" />
    </svg>
  );
}

function Counter({ to, label, count }: { to: string; label: string; count: number }) {
  return (
    <Link to={to} className={`group flex items-baseline gap-2 ${chip}`}>
      <span className="text-lg font-semibold tabular-nums">{count}</span>
      <span className="text-base text-muted group-hover:text-text">{label}</span>
    </Link>
  );
}

function EmptyState() {
  return (
    <section className="grid justify-items-center gap-3 py-16 text-center">
      <h2 className="text-2xl font-semibold text-balance">No projects yet</h2>
      <p className="max-w-[36ch] text-muted">
        Run <code className="font-mono text-sm">percept init</code> in a repository to start recording its sessions.
      </p>
      <a href={README} className={`mt-2 ${textLink}`}>
        Set up a client
      </a>
    </section>
  );
}
