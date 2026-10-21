import { Fragment } from "react";

import { getData, type MapDetail, type MapEdge, type MapNode } from "../api";
import { BackLink } from "../components/back-link";
import { Chevron } from "../components/chevron";
import { References } from "../components/references";
import { RouteError } from "../components/route-error";
import { textLink } from "../components/ui/text-link";
import { byDay, changedWhen, dayAndTime, full, time, withChangedTimes } from "../format";

export function loadMap(project: string, kind: string) {
  return getData<MapDetail>(`/api/projects/${encodeURIComponent(project)}/maps/${encodeURIComponent(kind)}`);
}

export function MapPage({ map }: { map: MapDetail }) {
  const now = new Date();
  const days = byDay(map.nodes, (n) => n.created_at, now);
  const nodes = new Map(map.nodes.map((n) => [n.id, n]));

  return (
    <>
      <h1 className="mt-8 sm:mt-10">
        <BackLink to="/" project={map.project} collection={map.kind}>
          <span className="tabular-nums"> · {map.nodes.length}</span>
        </BackLink>
      </h1>

      {map.nodes.length > 0 ? (
        <div className="mt-8 grid gap-8 sm:mt-10 sm:gap-10">
          {days.map((day) => (
            <section key={day.items[0].id}>
              <h2 className="mb-1 text-sm font-semibold text-muted">{day.name}</h2>
              <ul className="-mx-3 grid gap-0.5">
                {withChangedTimes(day.items, (n) => stamp(n, day.name, now)).map(({ item, shown }) => (
                  <NodeRow key={item.id} project={map.project} node={item} links={links(item, map.edges, nodes)} shown={shown} now={now} />
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

/** The time a node's row shows: when it was added, or when it changed, in the section headed `day`. */
function stamp(node: MapNode, day: string, now: Date) {
  return node.changed_at ? `changed ${changedWhen(new Date(node.changed_at), day, now)}` : time.format(new Date(node.created_at));
}

type Link = { kind: string; arrow: "→" | "←"; node: MapNode };

/** The edges at `node`, each with the node at its other end: `→` where it leaves, `←` where it arrives. */
function links(node: MapNode, edges: MapEdge[], nodes: Map<string, MapNode>): Link[] {
  return edges.flatMap((e): Link[] => {
    if (e.from === node.id) return [{ kind: e.kind, arrow: "→", node: nodes.get(e.to)! }];
    if (e.to === node.id) return [{ kind: e.kind, arrow: "←", node: nodes.get(e.from)! }];
    return [];
  });
}

/** Opens a node's row and moves focus to it, which scrolls it into view. */
function reveal(id: string) {
  const details = document.getElementById(`node-${id}`)?.querySelector("details");
  if (!details) return;

  details.open = true;
  details.querySelector("summary")?.focus();
}

function NodeRow({ project, node, links, shown, now }: { project: string; node: MapNode; links: Link[]; shown?: string; now: Date }) {
  const properties = Object.entries(node.properties);
  const kinds = [...new Set(links.map((l) => l.kind))];
  const created = new Date(node.created_at);
  const changed = node.changed_at ? new Date(node.changed_at) : null;
  const at = changed ?? created;
  const row = "flex min-h-11 items-start gap-2.5 rounded-lg px-3 py-2.5";
  const content = (
    <>
      <span className="min-w-0 flex-1 wrap-anywhere">{node.title}</span>
      {shown && (
        <time dateTime={at.toISOString()} title={full.format(at)} className="flex-none text-sm leading-6 text-muted tabular-nums">
          {shown}
        </time>
      )}
    </>
  );

  if (properties.length === 0 && links.length === 0 && node.references.length === 0 && !changed) {
    return (
      <li id={`node-${node.id}`} className={row}>
        <span className="size-3.5 flex-none" aria-hidden />
        {content}
      </li>
    );
  }

  return (
    <li id={`node-${node.id}`}>
      <details className="group">
        <summary className={`${row} cursor-pointer list-none hover:bg-surface focus-visible:bg-surface [&::-webkit-details-marker]:hidden`}>
          <Chevron className="mt-1 text-muted" />
          {content}
        </summary>
        <div className="grid gap-3.5 pt-1 pr-3 pb-5 pl-9">
          {changed && (
            <p className="text-sm text-muted tabular-nums">
              Added {dayAndTime(created, now)} · Changed {dayAndTime(changed, now)}
            </p>
          )}
          {(properties.length > 0 || links.length > 0) && (
            <dl className="grid gap-3.5">
              {properties.map(([key, value]) => (
                <div key={key}>
                  <dt className="text-sm font-medium text-muted">{key}</dt>
                  <dd className="mt-0.5 leading-relaxed wrap-anywhere">{value}</dd>
                </div>
              ))}
              {kinds.map((kind) => (
                <div key={`edge-${kind}`}>
                  <dt className="text-sm font-medium text-muted">{kind}</dt>
                  {(["→", "←"] as const).map((arrow) => {
                    const ends = links.filter((l) => l.kind === kind && l.arrow === arrow);

                    return (
                      ends.length > 0 && (
                        <dd key={arrow} className="mt-0.5 leading-relaxed wrap-anywhere">
                          <span className="text-muted">{arrow} </span>
                          {ends.map((l, i) => (
                            <Fragment key={l.node.id}>
                              {i > 0 && " · "}
                              <button type="button" onClick={() => reveal(l.node.id)} className={`${textLink} cursor-pointer text-left`}>
                                {l.node.title}
                              </button>
                            </Fragment>
                          ))}
                        </dd>
                      )
                    );
                  })}
                </div>
              ))}
            </dl>
          )}
          {node.references.length > 0 && <References project={project} references={node.references} />}
        </div>
      </details>
    </li>
  );
}

export function MapError() {
  return <RouteError notFound="Map not found." failed="map" />;
}
