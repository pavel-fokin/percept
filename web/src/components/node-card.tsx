import { useMemo, useState } from "react";
import { pathTo } from "../lib/outline";
import type { Outline } from "../lib/outline";
import { CloseIcon, KindGlyph } from "./icons";
import type { MapEdge } from "../lib/types";

/** One selected node, its own component so a narrow viewport's bottom
 * sheet can show the same card the wide layout's side panel does. */
export default function NodeCard({
  id,
  edges,
  outline,
  onSelect,
  onClose,
}: {
  id: string;
  edges: MapEdge[];
  outline: Outline;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const node = outline.byId.get(id);

  const crumbs = useMemo(() => pathTo(id, outline.parent).slice(0, -1), [id, outline]);
  const down = useMemo(() => outline.children.get(id) ?? [], [outline, id]);

  // Every parent but the first-reach one the outline already shows -
  // where else this node is pointed to from.
  const others = useMemo(() => {
    const primary = outline.parent.get(id);
    const seen = new Set<string>();
    for (const edge of edges) {
      if (edge.to === id && edge.from !== primary) seen.add(edge.from);
    }
    return [...seen];
  }, [edges, id, outline]);

  const groups = useMemo(() => {
    const byKind = new Map<string, MapEdge[]>();
    for (const edge of down) {
      const list = byKind.get(edge.kind);
      if (list) list.push(edge);
      else byKind.set(edge.kind, [edge]);
    }
    return [...byKind];
  }, [down]);

  const [copied, setCopied] = useState(false);

  if (!node) return null;

  function copyId() {
    navigator.clipboard
      .writeText(node!.id)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1000);
      })
      .catch(() => {});
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={onClose}
        aria-label="Close"
        className="absolute right-0 top-0 flex size-11 items-center justify-center text-muted hover:text-ink"
      >
        <CloseIcon className="size-4" />
      </button>

      <div className="flex flex-wrap items-center gap-x-1 gap-y-1 pr-11 text-[0.75rem] text-faint">
        {crumbs.length > 0 ? (
          <>
            {crumbs.map((crumbId) => (
              <span key={crumbId} className="flex items-center gap-1">
                <button type="button" onClick={() => onSelect(crumbId)} className="min-h-11 hover:text-accent">
                  {outline.byId.get(crumbId)?.name ?? crumbId}
                </button>
                <span aria-hidden="true">&#8250;</span>
              </span>
            ))}
            <span className="text-muted">{node.name}</span>
          </>
        ) : (
          <span>heads the map</span>
        )}
      </div>

      {others.length > 0 && (
        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[0.75rem] text-faint">
          <span>also under</span>
          {others.map((otherId) => (
            <button
              key={otherId}
              type="button"
              onClick={() => onSelect(otherId)}
              className="min-h-11 text-muted hover:text-accent"
            >
              {outline.byId.get(otherId)?.name ?? otherId}
            </button>
          ))}
        </div>
      )}

      <div className="mt-3 flex items-center gap-2 text-[0.75rem] text-faint">
        {outline.multi && (
          <span className="inline-flex items-center gap-1.5 text-muted">
            <KindGlyph index={outline.kindIndex.get(node.kind) ?? 0} className="size-3 text-faint" />
            {node.kind}
          </span>
        )}
        <button
          type="button"
          onClick={copyId}
          className="rounded-sm border border-rule px-1.5 py-0.5 font-mono text-[0.6875rem] text-faint hover:border-faint hover:text-ink"
        >
          {copied ? "copied" : node.id}
        </button>
      </div>

      <h2 className="mt-1 font-serif text-[1.375rem] font-medium leading-snug text-ink">{node.name}</h2>

      {Object.keys(node.properties).length > 0 && (
        <dl className="mt-3 grid gap-2.5">
          {Object.entries(node.properties).map(([key, value]) => (
            <div key={key}>
              <dt className="text-[0.6875rem] uppercase tracking-wide text-faint">{key}</dt>
              <dd className="mt-0.5 font-serif text-[1rem] leading-relaxed text-ink">{value}</dd>
            </div>
          ))}
        </dl>
      )}

      {down.length > 0 && (
        <div className="mt-5">
          <h3 className="flex items-center justify-between text-[0.6875rem] font-medium uppercase tracking-wide text-faint">
            <span>Under it</span>
            {groups.length === 1 && <span>{groups[0][0]}</span>}
          </h3>
          {groups.map(([kind, list]) => (
            <div key={kind}>
              {groups.length > 1 && <p className="pt-2 text-[0.75rem] text-faint">{kind}</p>}
              <ul className="border-t border-rule">
                {list.map((edge) => {
                  const target = outline.byId.get(edge.to);
                  return (
                    <li key={edge.to} className="border-b border-rule">
                      <button
                        type="button"
                        onClick={() => onSelect(edge.to)}
                        className="flex min-h-11 w-full items-center gap-2 py-2.5 text-left text-[0.875rem] text-ink hover:text-accent"
                      >
                        {outline.multi && (
                          <KindGlyph
                            index={outline.kindIndex.get(target?.kind ?? "") ?? 0}
                            className="size-3 shrink-0 text-faint"
                          />
                        )}
                        <span className="min-w-0 flex-1">{target?.name ?? edge.to}</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
