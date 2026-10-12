import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type MapSummary, type Session } from "../api";
import { textLink } from "../components/ui/text-link";
import { pathUrl } from "../paths";

const README = "https://github.com/pavel-fokin/percept#readme";

type PathGroup = { path: string; maps: MapSummary[]; sessions: number };

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

  for (const session of sessions) group(session.path).sessions += 1;
  for (const map of maps) group(map.path).maps.push(map);

  return [...groups.values()];
}

export function Start() {
  const groups = useLoaderData<typeof loadStart>();

  if (groups.length === 0) return <EmptyState />;

  return (
    <ul className="pt-2">
      {groups.map(({ path, maps, sessions }) => (
        <li key={path} className="border-t border-border py-4 first:border-t-0">
          <h2 className="font-mono text-sm">{path}</h2>
          <div className="mt-1 flex flex-wrap gap-x-6 gap-y-1">
            {maps.map((map) => (
              <Counter key={map.id} to={`/maps/${map.id}`} label={map.kind} count={map.nodes} />
            ))}
            {sessions > 0 && <Counter to={pathUrl(path)} label="sessions" count={sessions} />}
          </div>
        </li>
      ))}
    </ul>
  );
}

export function StartError() {
  return <p className="pt-6">Failed to load sessions: {describe(useRouteError())}</p>;
}

function Counter({ to, label, count }: { to: string; label: string; count: number }) {
  return (
    <Link to={to} className={`text-sm ${textLink}`}>
      {label} <span className="tabular-nums">{count}</span>
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
