import { Link, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type Session } from "../api";
import { textLink } from "../components/ui/text-link";
import { pathUrl } from "../paths";

const README = "https://github.com/pavel-fokin/percept#readme";

/** One group per path, in order of each path's newest session. */
export async function loadStart() {
  const sessions = await getData<Session[]>("/api/sessions");

  const counts = new Map<string, number>();
  for (const session of sessions) counts.set(session.path, (counts.get(session.path) ?? 0) + 1);

  return [...counts].map(([path, count]) => ({ path, count }));
}

export function Start() {
  const paths = useLoaderData<typeof loadStart>();

  if (paths.length === 0) return <EmptyState />;

  return (
    <ul className="pt-2">
      {paths.map(({ path, count }) => (
        <li key={path} className="border-t border-border py-4 first:border-t-0">
          <h2 className="font-mono text-sm">{path}</h2>
          <div className="mt-1 flex gap-6">
            <Counter to={pathUrl(path)} label="sessions" count={count} />
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
