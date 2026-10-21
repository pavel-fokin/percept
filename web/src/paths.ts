export type Route =
  | { page: "sessions"; project: string }
  | { page: "session"; project: string; id: string }
  | { page: "map"; project: string; kind: string };

const url = (...segments: string[]) => `/${["home", ...segments].map(encodeURIComponent).join("/")}`;

export const sessionsUrl = (project: string) => url(project, "sessions");
export const sessionUrl = (project: string, id: string) => url(project, "sessions", id);
export const mapUrl = (project: string, kind: string) => url(project, "maps", kind);

/** The page a `*` splat names, or null if it names none. */
export function parseRoute(splat: string): Route | null {
  const [base, project, view, rest, ...extra] = splat.split("/");
  if (base !== "home" || !project || extra.length > 0) return null;

  if (view === "sessions" && rest === undefined) return { page: "sessions", project };
  if (view === "sessions" && rest) return { page: "session", project, id: rest };
  if (view === "maps" && rest) return { page: "map", project, kind: rest };
  return null;
}
