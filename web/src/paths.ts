export type Route =
  | { page: "sessions"; project: string }
  | { page: "session"; project: string; id: string }
  | { page: "map"; project: string; name: string };

const url = (...segments: string[]) => `/${["home", ...segments].map(encodeURIComponent).join("/")}`;

export const sessionsUrl = (project: string) => url(project, "sessions");
export const sessionUrl = (project: string, id: string) => url(project, "sessions", id);
export const mapUrl = (project: string, name: string) => url(project, "maps", name);
export const boardUrl = (project: string, board: string) => url(project, "boards", board);

/** The router pattern of a board page, which sits outside the reading column. */
export const boardPattern = "home/:project/boards/:board";

/** The page a `*` splat names, or null if it names none. */
export function parseRoute(splat: string): Route | null {
  const [base, project, view, rest, ...extra] = splat.split("/");
  if (base !== "home" || !project || extra.length > 0) return null;

  if (view === "sessions" && rest === undefined) return { page: "sessions", project };
  if (view === "sessions" && rest) return { page: "session", project, id: rest };
  if (view === "maps" && rest) return { page: "map", project, name: rest };
  return null;
}
