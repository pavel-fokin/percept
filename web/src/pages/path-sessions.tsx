import { useLoaderData, type LoaderFunctionArgs } from "react-router";
import { getData, type Session } from "../api";
import { RouteError } from "../components/route-error";
import { SessionRow } from "../components/session-row";
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
  return <RouteError notFound="No sessions recorded at this path." failed="sessions" />;
}
