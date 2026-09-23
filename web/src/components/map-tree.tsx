import { useCallback, useEffect, useRef, useState } from "react";
import { pathTo } from "../lib/outline";
import type { Outline, OutlineEntry } from "../lib/outline";
import { count } from "../lib/format";
import { Chevron, KindGlyph } from "./icons";
import type { MapKind } from "../lib/types";

const INDENT = 20;
const LEAD = 40;

/** A cognitive map as a collapsible outline: one tree per head, walked
 * from `outline`, so opening a branch never waits on a round trip.
 * `selected` names the node the card shows - set from outside, by a
 * row here, a back-reference, or the card itself - and every path to
 * it opens the same way: expand its ancestors, then scroll and flash
 * its row once it exists. */
export default function MapTree({
  kinds,
  outline,
  selected,
  onSelect,
}: {
  kinds: MapKind[];
  outline: Outline;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(outline.forest.map((node) => node.id)));

  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const rows = useRef(new Map<string, HTMLDivElement | null>());

  // Opening the branches down to a node happens in one render;
  // scrolling to its row needs the next one, once that row exists.
  const reveal = useCallback(
    (id: string) => {
      const ancestors = pathTo(id, outline.parent).slice(0, -1);
      setExpanded((prev) => (ancestors.every((a) => prev.has(a)) ? prev : new Set([...prev, ...ancestors])));
      setScrollTo(id);
    },
    [outline],
  );

  useEffect(() => {
    if (selected && outline.byId.has(selected)) reveal(selected);
  }, [selected, outline, reveal]);

  // A click here reveals its node itself: choosing the node already
  // selected leaves `selected` unchanged, so the effect above would not
  // bring back a row a collapse has since hidden.
  function choose(id: string) {
    reveal(id);
    onSelect(id);
  }

  useEffect(() => {
    if (!scrollTo) return;
    setScrollTo(null);
    const row = rows.current.get(scrollTo);
    row?.scrollIntoView({ behavior: "smooth", block: "nearest" });
    if (row && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      row.animate(
        [
          { backgroundColor: "color-mix(in oklab, var(--accent) 22%, transparent)" },
          { backgroundColor: "transparent" },
        ],
        { duration: 900, easing: "ease-out" },
      );
    }
  }, [scrollTo, expanded]);

  function toggle(id: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const allOpen = outline.branches.size > 0 && [...outline.branches].every((id) => expanded.has(id));

  const register = useCallback((id: string, el: HTMLDivElement | null) => {
    rows.current.set(id, el);
  }, []);

  return (
    <div>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
        {outline.multi ? (
          <ul className="flex flex-wrap gap-x-4 gap-y-1 text-[0.8125rem] text-muted">
            {kinds.map((kind, index) => (
              <li key={kind.kind} className="flex items-center gap-1.5">
                <KindGlyph index={index} className="size-3 text-faint" />
                {kind.kind}
              </li>
            ))}
          </ul>
        ) : (
          <span />
        )}
        <button
          type="button"
          onClick={() =>
            setExpanded(allOpen ? new Set(outline.forest.map((node) => node.id)) : new Set(outline.branches))
          }
          className="inline-flex min-h-11 items-center text-[0.8125rem] text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
        >
          {allOpen ? "Collapse all" : "Expand all"}
        </button>
      </div>

      {outline.forest.length > 0 && (
        <ul role="tree" className="mt-2 border-t border-rule">
          {outline.forest.map((node, index) => (
            <Row
              key={`${index}:${node.id}`}
              entry={node}
              path={`${index}:${node.id}`}
              outline={outline}
              expanded={expanded}
              selected={selected}
              onToggle={toggle}
              onSelect={choose}
              register={register}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function Row({
  entry,
  path,
  outline,
  expanded,
  selected,
  onToggle,
  onSelect,
  register,
}: {
  entry: OutlineEntry;
  path: string;
  outline: Outline;
  expanded: Set<string>;
  selected: string | null;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
  register: (id: string, el: HTMLDivElement | null) => void;
}) {
  const node = outline.byId.get(entry.id);
  const name = node?.name ?? entry.id;
  const lead = entry.depth * INDENT + LEAD;

  if ("ref" in entry) {
    return (
      <li role="none">
        <button
          type="button"
          onClick={() => onSelect(entry.id)}
          role="treeitem"
          className="flex w-full items-center border-t border-rule text-left hover:bg-panel"
        >
          <span style={{ width: lead }} className="flex h-11 shrink-0 items-center justify-end pr-1.5 text-faint">
            &#8617;
          </span>
          {outline.multi && (
            <KindGlyph index={outline.kindIndex.get(node?.kind ?? "") ?? 0} className="size-3 shrink-0 text-faint" />
          )}
          <span className="min-w-0 flex-1 py-2.5 text-[0.9375rem] text-muted">{name}</span>
          <span className="mr-2 shrink-0 text-xs text-faint">shown above</span>
        </button>
      </li>
    );
  }

  const has = entry.kids.length > 0;
  const open = expanded.has(entry.id);
  const isSelected = selected === entry.id;

  return (
    <li role="none">
      <div
        ref={(el) => register(entry.id, el)}
        role="treeitem"
        aria-expanded={has ? open : undefined}
        aria-selected={isSelected}
        className={"relative flex items-center border-t border-rule" + (isSelected ? " bg-panel" : "")}
      >
        {isSelected && <span className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-accent" />}
        <button
          type="button"
          onClick={() => has && onToggle(entry.id)}
          tabIndex={has ? 0 : -1}
          aria-hidden={!has}
          style={{ width: lead }}
          className="flex h-11 shrink-0 items-center justify-end pr-1.5 text-faint"
        >
          {has ? (
            <Chevron className={"size-3 transition-transform" + (open ? "" : " -rotate-90")} />
          ) : (
            <span className="size-1 rounded-full bg-rule" />
          )}
        </button>
        {outline.multi && (
          <KindGlyph index={outline.kindIndex.get(node?.kind ?? "") ?? 0} className="size-3 shrink-0 text-faint" />
        )}
        <button
          type="button"
          onClick={() => onSelect(entry.id)}
          className={
            "min-w-0 flex-1 py-2.5 text-left text-[0.9375rem] text-ink" + (entry.depth === 0 ? " font-medium" : "")
          }
        >
          {name}
        </button>
        {has && !open && <span className="mr-2 shrink-0 text-xs tabular-nums text-faint">{count(entry.size)}</span>}
      </div>
      {has && open && (
        <ul role="group">
          {entry.kids.map((kid, index) => (
            <Row
              key={`${path}/${index}:${kid.id}`}
              entry={kid}
              path={`${path}/${index}:${kid.id}`}
              outline={outline}
              expanded={expanded}
              selected={selected}
              onToggle={onToggle}
              onSelect={onSelect}
              register={register}
            />
          ))}
        </ul>
      )}
    </li>
  );
}
