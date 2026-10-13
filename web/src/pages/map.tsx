import { useState } from "react";
import { useLoaderData, type LoaderFunctionArgs } from "react-router";
import { getData, type MapDetail, type MapNode } from "../api";
import { RelativeTime } from "../components/relative-time";
import { RouteError } from "../components/route-error";

export function loadMap({ params }: LoaderFunctionArgs) {
  return getData<MapDetail>(`/api/maps/${params.id}`);
}

function NodeRow({ node, now }: { node: MapNode; now: Date }) {
  const [open, setOpen] = useState(false);
  const properties = Object.entries(node.properties);

  const row = "-mx-3 flex min-h-11 w-[calc(100%+1.5rem)] items-baseline justify-between gap-6 rounded-md p-3 text-left";
  const content = (
    <>
      <span className="min-w-0 flex-1 text-base md:text-sm">{node.title}</span>
      <RelativeTime at={node.created_at} now={now} />
    </>
  );

  // The rail runs from this node's dot to the next one's, so the last node has none.
  return (
    <li className="relative pb-1 pl-6 not-last:before:absolute not-last:before:top-5 not-last:before:-bottom-5 not-last:before:left-[3px] not-last:before:w-px not-last:before:bg-border">
      <span className="absolute top-4 left-0 size-[7px] rounded-full bg-muted" aria-hidden />
      {properties.length > 0 ? (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className={`${row} cursor-pointer hover:bg-surface focus-visible:bg-surface`}
        >
          {content}
        </button>
      ) : (
        <div className={row}>{content}</div>
      )}
      {open && (
        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 pb-3 text-sm">
          {properties.map(([key, value]) => (
            <div key={key} className="contents">
              <dt className="text-muted">{key}</dt>
              <dd className="min-w-0 break-words">{value}</dd>
            </div>
          ))}
        </dl>
      )}
    </li>
  );
}

export function MapPage() {
  const map = useLoaderData<typeof loadMap>();
  const now = new Date();

  return (
    <>
      <h1 className="pt-6 text-lg font-semibold">{map.kind}</h1>
      <p className="pb-2 font-mono text-sm text-muted">{map.path}</p>
      {map.nodes.length > 0 ? (
        <ol className="pt-2">
          {map.nodes.map((node) => (
            <NodeRow key={node.id} node={node} now={now} />
          ))}
        </ol>
      ) : (
        <section className="grid justify-items-center gap-3 py-16 text-center">
          <h3 className="text-2xl font-semibold text-balance">No {map.kind} yet</h3>
          <p className="max-w-[36ch] text-muted">
            <span className="capitalize">{map.kind}</span> appear here once an agent records one in this project.
          </p>
        </section>
      )}
    </>
  );
}

export function MapError() {
  return <RouteError notFound="Map not found." failed="map" />;
}
