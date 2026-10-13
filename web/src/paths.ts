export type Route =
  | { page: "sessions"; path: string }
  | { page: "session"; path: string; id: string }
  | { page: "map"; path: string; id: string };

/** `~/x` becomes `/home/x/…`, `/x` becomes `/root/x/…`. */
function projectUrl(path: string, ...tail: string[]) {
  const [base, rest] = path.startsWith("~") ? ["home", path.slice(1)] : ["root", path];
  const segments = [base, ...rest.split("/").filter(Boolean), ...tail];
  return `/${segments.map(encodeURIComponent).join("/")}`;
}

export const sessionsUrl = (path: string) => projectUrl(path, "sessions");
export const sessionUrl = (path: string, id: string) => projectUrl(path, "sessions", id);
export const mapUrl = (path: string, id: string) => projectUrl(path, "maps", id);

/** The page a `*` splat names, or null if it names none. Read from the end, so a folder called `sessions` stays a folder. */
export function parseRoute(splat: string): Route | null {
  const [base, ...segments] = splat.split("/");
  if (base !== "home" && base !== "root") return null;

  const pathOf = (rest: string[]) => (base === "home" ? ["~", ...rest].join("/") : `/${rest.join("/")}`);
  const [view, id] = segments.slice(-2);

  if (segments.at(-1) === "sessions") return { page: "sessions", path: pathOf(segments.slice(0, -1)) };
  if (id && view === "sessions") return { page: "session", path: pathOf(segments.slice(0, -2)), id };
  if (id && view === "maps") return { page: "map", path: pathOf(segments.slice(0, -2)), id };
  return null;
}
