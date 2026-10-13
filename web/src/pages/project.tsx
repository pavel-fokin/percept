import { useLoaderData, useParams, type LoaderFunctionArgs } from "react-router";
import { parseRoute } from "../paths";
import { loadMap, MapError, MapPage } from "./map";
import { loadPathSessions, PathSessions, PathSessionsError } from "./path-sessions";
import { loadSession, SessionError, SessionPage } from "./session";

type Loaded =
  | { page: "sessions"; data: Awaited<ReturnType<typeof loadPathSessions>> }
  | { page: "session"; data: Awaited<ReturnType<typeof loadSession>> }
  | { page: "map"; data: Awaited<ReturnType<typeof loadMap>> };

/** The one route under a project's path: its URL picks the page. */
export async function loadProject({ params }: LoaderFunctionArgs): Promise<Loaded> {
  const route = parseRoute(params["*"] ?? "");
  if (route === null) throw new Response(null, { status: 404 });

  switch (route.page) {
    case "sessions":
      return { page: "sessions", data: await loadPathSessions(route.path) };
    case "session":
      return { page: "session", data: await loadSession(route.path, route.id) };
    case "map":
      return { page: "map", data: await loadMap(route.path, route.id) };
  }
}

export function Project() {
  const loaded = useLoaderData<typeof loadProject>();

  switch (loaded.page) {
    case "sessions":
      return <PathSessions {...loaded.data} />;
    case "session":
      return <SessionPage {...loaded.data} />;
    case "map":
      return <MapPage map={loaded.data} />;
  }
}

export function ProjectError() {
  const page = parseRoute(useParams()["*"] ?? "")?.page;

  if (page === "session") return <SessionError />;
  if (page === "map") return <MapError />;
  return <PathSessionsError />;
}
