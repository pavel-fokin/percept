import { useLoaderData, type LoaderFunctionArgs } from "react-router";
import { getData, type MapDetail, type MapNode } from "../api";
import { RouteError } from "../components/route-error";
import { byDay, full, withChangedTimes } from "../format";

export function loadMap({ params }: LoaderFunctionArgs) {
  return getData<MapDetail>(`/api/maps/${params.id}`);
}

export function MapPage() {
  const map = useLoaderData<typeof loadMap>();
  const days = byDay(map.nodes, (n) => n.created_at, new Date());

  return (
    <>
      <div className="mt-8 sm:mt-10">
        <p className="text-sm text-muted">{map.path}</p>
        <h1 className="mt-1.5 flex items-baseline gap-2.5 text-2xl font-semibold tracking-tight capitalize sm:text-[26px]">
          {map.kind}
          <span className="text-[17px] font-normal tracking-normal text-muted tabular-nums">{map.nodes.length}</span>
        </h1>
      </div>

      {map.nodes.length > 0 ? (
        <div className="mt-8 grid gap-8 sm:mt-10 sm:gap-10">
          {days.map((day) => (
            <section key={day.items[0].id}>
              <h2 className="mb-1 text-sm font-semibold text-muted">{day.name}</h2>
              <ul className="-mx-3 grid gap-0.5">
                {withChangedTimes(day.items, (n) => n.created_at).map(({ item, shown }) => (
                  <NodeRow key={item.id} node={item} shown={shown} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      ) : (
        <section className="grid justify-items-center gap-3 py-16 text-center">
          <h2 className="text-2xl font-semibold text-balance">No {map.kind} yet</h2>
          <p className="max-w-[36ch] text-muted">
            <span className="capitalize">{map.kind}</span> appear here once an agent records one in this project.
          </p>
        </section>
      )}
    </>
  );
}

function NodeRow({ node, shown }: { node: MapNode; shown?: string }) {
  const properties = Object.entries(node.properties);
  const row = "flex min-h-11 items-start gap-2.5 rounded-[10px] px-3 py-2.5";
  const content = (
    <>
      <span className="min-w-0 flex-1 wrap-anywhere">{node.title}</span>
      {shown && (
        <time dateTime={node.created_at} title={full.format(new Date(node.created_at))} className="flex-none text-[13px] leading-6 text-muted tabular-nums">
          {shown}
        </time>
      )}
    </>
  );

  if (properties.length === 0) {
    return (
      <li className={row}>
        <span className="size-3.5 flex-none" aria-hidden />
        {content}
      </li>
    );
  }

  return (
    <li>
      <details className="group">
        <summary className={`${row} cursor-pointer list-none hover:bg-surface focus-visible:bg-surface [&::-webkit-details-marker]:hidden`}>
          <svg
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            aria-hidden
            className="mt-[5px] size-3.5 flex-none text-muted transition-transform group-open:rotate-90"
          >
            <path d="m6 3 5 5-5 5" />
          </svg>
          {content}
        </summary>
        <dl className="grid gap-3.5 pt-1 pr-3 pb-5 pl-9">
          {properties.map(([key, value]) => (
            <div key={key}>
              <dt className="text-[13px] font-medium text-muted">{key}</dt>
              <dd className="mt-0.5 leading-[26px] wrap-anywhere">{value}</dd>
            </div>
          ))}
        </dl>
      </details>
    </li>
  );
}

export function MapError() {
  return <RouteError notFound="Map not found." failed="map" />;
}
