import { isRouteErrorResponse, Link, useLoaderData, useRouteError, type LoaderFunctionArgs } from "react-router";
import { describe, getData, type Session } from "../api";
import { SessionRow } from "../components/session-row";
import { textLink } from "../components/ui/text-link";
import { urlPath } from "../paths";

export async function loadPathSessions({ params }: LoaderFunctionArgs) {
  const path = urlPath(params["*"] ?? "");
  if (path === null) throw new Response(null, { status: 404 });

  const sessions = (await getData<Session[]>("/api/sessions")).filter((s) => s.path === path);
  if (sessions.length === 0) throw new Response(null, { status: 404 });

  return { path, sessions };
}

export function PathSessions() {
  const { path, sessions } = useLoaderData<typeof loadPathSessions>();
  const now = new Date();

  return (
    <>
      <h2 className="pt-6 pb-2 font-mono text-sm font-semibold text-muted">{path}</h2>
      <ul>
        {sessions.map((session) => (
          <SessionRow key={session.id} session={session} now={now} />
        ))}
      </ul>
    </>
  );
}

export function PathSessionsError() {
  const error = useRouteError();

  if (isRouteErrorResponse(error) && error.status === 404) {
    return (
      <p className="pt-6">
        No sessions recorded at this path.{" "}
        <Link to="/" className={textLink}>
          All paths
        </Link>
      </p>
    );
  }

  return <p className="pt-6">Failed to load sessions: {describe(error)}</p>;
}
