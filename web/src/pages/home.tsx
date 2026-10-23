import { useEffect, useState } from "react";
import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type BoardSummary, type MapSummary, type Project, type Session } from "../api";
import { relative } from "../format";
import { BoardIcon } from "../components/board-icon";
import { NewBoard } from "../components/new-board";
import { NewProjectForm } from "../components/new-project-form";
import { Spinner } from "../components/spinner";
import { Button } from "../components/ui/button";
import { chip } from "../components/ui/chip";
import { textLink } from "../components/ui/text-link";
import { boardUrl, mapUrl, sessionsUrl, sessionUrl } from "../paths";
import { usePolling } from "../use-polling";

const README = "https://github.com/pavel-fokin/percept#readme";

type ProjectGroup = { project: string; root: string | null; maps: MapSummary[]; boards: BoardSummary[]; sessions: number; latest?: Session };

const hasContent = (g: Pick<ProjectGroup, "maps" | "boards">) => g.maps.length > 0 || g.boards.length > 0;

/** One group per project: those with sessions by newest session, then those with only maps or boards, then empty ones. */
export async function loadHome() {
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

export function Home() {
  const groups = useLoaderData<typeof loadHome>();
  usePolling(true);
  const now = new Date();

  if (groups.length === 0) return <EmptyState />;

  return (
    <div className="mt-8 grid gap-14 sm:mt-14">
      {groups.map(({ project, root, maps, boards, sessions, latest }) => {
        const isEmpty = sessions === 0 && !hasContent({ maps, boards });
        // Sessions reach only a project with a folder, so a project without one gets no hint.
        const awaitsSessions = isEmpty && root !== null;

        return (
          <section key={project}>
            <div className="flex items-baseline justify-between gap-4">
              <h2 className="min-w-0 font-semibold wrap-anywhere">{project}</h2>
              {latest && (
                <time dateTime={latest.last_event_at} className="flex-none text-sm text-muted tabular-nums">
                  active {relative(new Date(latest.last_event_at), now)}
                </time>
              )}
              {awaitsSessions && <span className="flex-1 text-right font-mono text-sm text-muted wrap-anywhere">{root}</span>}
            </div>

            {awaitsSessions && (
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
                  <Counter key={map.name} to={mapUrl(project, map.name)} label={map.name} count={map.nodes} />
                ))}
                {sessions > 0 && <Counter to={sessionsUrl(project)} label="sessions" count={sessions} />}
              </nav>
            )}

            <nav aria-label={`${project} boards`} className="-mx-3 mt-1 flex flex-wrap items-center gap-1">
              {boards.map((board) => (
                <Link
                  key={board.id}
                  to={boardUrl(project, board.name)}
                  className={`group gap-2 ${chip}`}
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

export function HomeError() {
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
    <Link to={to} className={`group ${chip}`}>
      <span className="flex items-baseline gap-2">
        <span className="text-lg font-semibold tabular-nums">{count}</span>
        <span className="text-base text-muted group-hover:text-text">{label}</span>
      </span>
    </Link>
  );
}

function EmptyState() {
  const [editing, setEditing] = useState(false);

  return (
    <section className="grid justify-items-center gap-3 py-16 text-center">
      <h2 className="text-2xl font-semibold text-balance">No projects yet</h2>
      <p className="max-w-[36ch] text-muted">A project holds the maps and boards you think a question through on.</p>
      <div className="mt-2 w-full max-w-xs">
        {editing ? (
          <NewProjectForm onCreated={() => {}} onCancel={() => setEditing(false)} />
        ) : (
          <Button variant="primary" onClick={() => setEditing(true)}>
            New project
          </Button>
        )}
      </div>
      <p className="mt-6 max-w-[36ch] text-sm text-muted">Working in a repository? Run this there to record its coding sessions:</p>
      <CopyCommand command="percept init" />
    </section>
  );
}

// The clipboard API exists only on https and localhost; over a LAN address the command stays selectable text.
const canCopy = window.isSecureContext && "clipboard" in navigator;

function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const copy = async () => {
    await navigator.clipboard.writeText(command);
    setCopied(true);
  };

  if (!canCopy) return <code className="font-mono text-sm text-text select-all">{command}</code>;

  return (
    <>
      <Button variant="secondary" aria-label={`Copy ${command}`} onClick={() => void copy()} className="gap-6 font-mono">
        {command}
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="size-4 text-muted">
          {copied ? (
            <path d="M3 8.5l3.5 3.5L13 4.5" />
          ) : (
            <>
              <rect x="5.75" y="5.75" width="8" height="8" rx="1.5" />
              <path d="M10.25 3.5v-.25a1.5 1.5 0 0 0-1.5-1.5h-5a1.5 1.5 0 0 0-1.5 1.5v5a1.5 1.5 0 0 0 1.5 1.5h.25" />
            </>
          )}
        </svg>
      </Button>
      <span role="status" className="sr-only">
        {copied ? "Copied" : ""}
      </span>
    </>
  );
}
