import { useEffect, useState } from "react";
import { Link } from "react-router";
import { fetchProjects, messageOf } from "../lib/api";
import { count, relativeTime } from "../lib/format";
import { mapPath } from "../lib/routes";
import type { Project } from "../lib/types";

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; projects: Project[] };

/** The index: every project the log holds, the most recently active
 * first, each row carrying what percept keeps for it. It stands in
 * until the Attention screen exists, so it orients rather than
 * prioritises - it says what is here, not what is owed. */
export default function Projects() {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoad({ state: "loading" });
    fetchProjects()
      .then((response) => {
        if (!cancelled) setLoad({ state: "ready", projects: response.projects });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoad({ state: "failed", message: messageOf(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [retry]);

  return (
    <main id="projects" className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <h1 className="mt-4 flex min-h-6 items-baseline text-base font-medium tracking-tight text-ink">Projects</h1>

      <div aria-live="polite" className="mt-3 flex min-h-6 flex-wrap items-center gap-x-3 text-[0.8125rem] text-faint">
        {load.state === "loading" && <span>Reading the log&#8230;</span>}
        {load.state === "failed" && (
          <>
            <span className="text-ink">The log could not be read: {load.message}.</span>
            <button
              type="button"
              onClick={() => setRetry((attempt) => attempt + 1)}
              className="inline-flex min-h-11 items-center text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
            >
              Try again
            </button>
          </>
        )}
        {load.state === "ready" && (
          <span className={load.projects.length === 0 ? "text-[0.9375rem] text-muted" : undefined}>
            {load.projects.length === 0
              ? "No project has recorded anything yet."
              : `${count(load.projects.length)} ${load.projects.length === 1 ? "project" : "projects"}, most recently active first.`}
          </span>
        )}
      </div>

      {load.state === "ready" && load.projects.length > 0 && (
        <ul className="mt-4">
          {load.projects.map((project, index, all) => (
            <li
              key={project.path}
              className={"border-t border-rule py-3" + (index === all.length - 1 ? " border-b" : "")}
            >
              <ProjectRow project={project} />
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}

function ProjectRow({ project }: { project: Project }) {
  return (
    <>
      <div className="flex items-baseline gap-x-3">
        <h2 className="min-w-0 flex-1 truncate text-[0.9375rem] font-medium text-ink">{project.name}</h2>
        <span className="shrink-0 font-mono text-[0.8125rem] tabular-nums text-faint">{count(project.events)} events</span>
      </div>
      <div className="mt-0.5 flex items-baseline gap-x-3">
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-faint" title={project.path}>
          {project.path}
        </span>
        <span className="shrink-0 text-xs text-faint">{relativeTime(project.last_active)}</span>
      </div>

      {project.maps_error ? (
        <p className="mt-2 text-[0.8125rem] text-ink">The maps here could not be read: {project.maps_error}.</p>
      ) : project.maps.length === 0 ? (
        <p className="mt-2 text-[0.8125rem] text-faint">No map declared here.</p>
      ) : (
        <ul className="mt-2 space-y-1">
          {project.maps.map((map) => (
            <li key={map.id} className="flex flex-wrap items-baseline gap-x-2 text-[0.8125rem]">
              <Link
                to={mapPath(map.id, project.path)}
                className="inline-flex min-h-11 items-center text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
              >
                {map.name}
              </Link>
              <span className="text-faint">
                <span className="font-mono tabular-nums">{count(map.nodes)}</span> nodes
              </span>
              {map.gained > 0 && (
                <span className="text-accent">
                  <span className="font-mono tabular-nums">+{count(map.gained)}</span> since last session
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
