const VIEW = "sessions";

/** `~/x` becomes `/p/home/x/sessions`, `/x` becomes `/p/root/x/sessions`. */
export function pathUrl(path: string) {
  const [base, rest] = path.startsWith("~") ? ["home", path.slice(1)] : ["root", path];
  const segments = [base, ...rest.split("/").filter(Boolean), VIEW];
  return `/p/${segments.map(encodeURIComponent).join("/")}`;
}

/** The path a `/p/*` splat names, or null if it is not one. */
export function urlPath(splat: string) {
  const [base, ...rest] = splat.split("/");
  const view = rest.pop();
  if (view !== VIEW || (base !== "home" && base !== "root")) return null;

  return base === "home" ? ["~", ...rest].join("/") : `/${rest.join("/")}`;
}
