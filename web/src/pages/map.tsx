import { isRouteErrorResponse, Link, useLoaderData, useRouteError, type LoaderFunctionArgs } from "react-router";
import { describe, getData, type MapSummary } from "../api";
import { textLink } from "../components/ui/text-link";

export async function loadMap({ params }: LoaderFunctionArgs) {
  const map = (await getData<MapSummary[]>("/api/maps")).find((m) => m.id === params.id);
  if (map === undefined) throw new Response(null, { status: 404 });

  return map;
}

export function MapPage() {
  const map = useLoaderData<typeof loadMap>();

  return (
    <>
      <h2 className="pt-6 text-lg font-semibold">{map.kind}</h2>
      <p className="pb-2 font-mono text-sm text-muted">{map.path}</p>
      <section className="grid justify-items-center gap-3 py-16 text-center">
        <h3 className="text-2xl font-semibold text-balance">No {map.kind} yet</h3>
        <p className="max-w-[36ch] text-muted">
          <span className="capitalize">{map.kind}</span> appear here once an agent records one in this project.
        </p>
      </section>
    </>
  );
}

export function MapError() {
  const error = useRouteError();

  if (isRouteErrorResponse(error) && error.status === 404) {
    return (
      <p className="pt-6">
        Map not found.{" "}
        <Link to="/" className={textLink}>
          All paths
        </Link>
      </p>
    );
  }

  return <p className="pt-6">Failed to load map: {describe(error)}</p>;
}
