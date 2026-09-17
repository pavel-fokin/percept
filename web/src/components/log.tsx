import { useMemo, useState } from "react";
import EventRow from "./event-row";
import { eventsCountLabel, foldResults, groupByDay, groupRuns, timeOf } from "../lib/event-rows";
import FilterMenu from "./filter-menu";
import type { Filter } from "../lib/filters";
import { basename } from "../lib/format";
import { SEARCH_FIELD_ID } from "../lib/routes";
import {
  ACTOR_OPTIONS,
  KIND_OPTIONS,
  TIME_OPTIONS,
  actorsLabel,
  EMPTY_FILTER,
  isFilterActive,
  kindsLabel,
  searchFromFilter,
  timeLabel,
  toggleActor,
  toggleKind,
} from "../lib/filters";
import { SearchIcon } from "./icons";
import { useActiveSpeaker } from "../hooks/use-active-speaker";
import type { Event } from "../lib/types";

/** Which of the three filter menus is open - never more than one, so
 * opening one closes the others for free. */
type OpenMenu = "kind" | "actor" | "time" | null;

/** The screen's body: search and filters over the log, grouped by day
 * and, within a day, by the run of consecutive rows one speaker
 * produced. A tool's answer is not a row of its own - it sits inside
 * the call that caused it. */
export default function Log({
  project,
  events,
  carried,
  total,
  resultsQuery,
  updatedAt,
  loadState,
  loadError,
  loadingMore,
  pagingFailed,
  onRetry,
  onShowEarlier,
  filter,
  onFilterChange,
}: {
  project: string | null;
  events: Event[];
  carried: Event[];
  total: number;
  resultsQuery: string | null;
  updatedAt: number | null;
  loadState: "loading" | "ready" | "updating" | "failed";
  loadError: string | null;
  loadingMore: boolean;
  pagingFailed: string | null;
  onRetry: () => void;
  onShowEarlier: () => void;
  filter: Filter;
  onFilterChange: (filter: Filter) => void;
}) {
  const [openMenu, setOpenMenu] = useState<OpenMenu>(null);
  // Held across a re-render: the speaker under the pinned strip changes
  // at every run boundary on the way down the page, and regrouping the
  // whole log each time is the one thing here that costs anything.
  const days = useMemo(() => groupByDay(foldResults(events, carried)), [events, carried]);
  const speaking = useActiveSpeaker(events);
  const resultsCurrent = resultsQuery === searchFromFilter(filter);
  const earlier = resultsCurrent && loadState === "ready" && events.length < total;
  const filtered = isFilterActive(filter);
  const noMatches = resultsCurrent && loadState === "ready" && filtered && total === 0;

  return (
    <main id="log" className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <div className="mt-4 flex min-h-6 items-baseline gap-x-2">
        <h1 className="text-base font-medium tracking-tight text-ink">Event log</h1>
        {project && <p className="text-[0.8125rem] text-faint">· from {basename(project)}</p>}
      </div>
      <label htmlFor={SEARCH_FIELD_ID} className="sr-only">
        Search every event
      </label>
      <div className="relative mt-3">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-0 size-4 -translate-y-1/2 text-faint" />
        <input
          id={SEARCH_FIELD_ID}
          type="search"
          value={filter.q}
          onChange={(event) => onFilterChange({ ...filter, q: event.target.value })}
          placeholder="Search every event&#8230;"
          className="min-h-11 w-full border-0 border-b border-rule bg-transparent pr-0 pl-7 text-[0.9375rem] placeholder:text-faint focus:border-accent focus:outline-none"
        />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <FilterMenu
          chipLabel={kindsLabel(filter.kinds)}
          options={KIND_OPTIONS}
          isSelected={(value) => filter.kinds.includes(value)}
          onSelect={(value) => onFilterChange({ ...filter, kinds: toggleKind(filter.kinds, value) })}
          multi
          open={openMenu === "kind"}
          onOpenChange={(open) => setOpenMenu(open ? "kind" : null)}
        />
        <FilterMenu
          chipLabel={actorsLabel(filter.actors)}
          options={ACTOR_OPTIONS}
          isSelected={(value) => filter.actors.includes(value)}
          onSelect={(value) => onFilterChange({ ...filter, actors: toggleActor(filter.actors, value) })}
          multi
          open={openMenu === "actor"}
          onOpenChange={(open) => setOpenMenu(open ? "actor" : null)}
        />
        <FilterMenu
          chipLabel={timeLabel(filter.since)}
          options={TIME_OPTIONS}
          isSelected={(value) => filter.since === value}
          onSelect={(value) => onFilterChange({ ...filter, since: value })}
          multi={false}
          open={openMenu === "time"}
          onOpenChange={(open) => setOpenMenu(open ? "time" : null)}
        />
        {filtered && (
          <button
            type="button"
            onClick={() => onFilterChange(EMPTY_FILTER)}
            className="inline-flex min-h-11 items-center text-[0.8125rem] text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
          >
            Clear
          </button>
        )}
      </div>

      <div aria-live="polite" className="mt-2 flex min-h-6 flex-wrap items-center gap-x-3 text-[0.8125rem] text-faint">
        {loadState === "loading" && <span>Reading the log…</span>}
        {loadState === "updating" && (
          <span>{events.length > 0 ? `Updating… Showing ${events.length.toLocaleString("en-GB")} previous events.` : "Updating…"}</span>
        )}
        {loadState === "failed" && (
          <>
            <span className="text-ink">
              The log could not be updated{loadError ? `: ${loadError}` : ""}.
              {events.length > 0 && " Previous results remain below."}
            </span>
            <button
              type="button"
              onClick={onRetry}
              className="inline-flex min-h-11 items-center text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
            >
              Try again
            </button>
          </>
        )}
        {loadState === "ready" && (
          <>
            <span className={noMatches ? "text-[0.9375rem] text-muted" : undefined}>
              {noMatches ? "Nothing in the log matches that filter." : eventsCountLabel(events.length, total)}
            </span>
            {updatedAt !== null && <span>Updated {updatedTime(updatedAt)}.</span>}
          </>
        )}
      </div>

      {days.map((day, i) => (
          <section key={day.key}>
            <h2
            className={`${
              i === 0 ? "mt-7" : "mt-8"
            } sticky top-14 z-10 flex flex-wrap items-center gap-x-2 bg-page py-2 text-xs font-medium uppercase tracking-[0.09em] text-muted sm:top-16`}
          >
            <span>{day.label}</span>
            {speaking?.day === day.key && (
              <>
                <span className="text-rule">&#183;</span>
                <span className="text-faint">{speaking.speaker}</span>
              </>
            )}
          </h2>
            <ul className="mt-2">
              {groupRuns(day.rows).map((run, runIndex, runs) => (
                <li
                  key={runIndex}
                  data-speaker={run.speaker.label}
                  data-day={day.key}
                  className={"border-t border-rule py-4" + (runIndex === runs.length - 1 ? " border-b" : "")}
                >
                  <p className={`text-[0.6875rem] font-medium uppercase tracking-[0.1em] ${run.speaker.toneClass}`}>
                    {run.speaker.label}
                    <span className="font-mono font-normal text-faint"> · {timeOf(run.rows[0].event)}</span>
                    {project && differentPath(run.rows[0].event.source.path, project) && (
                      <span className="text-faint"> · {basename(run.rows[0].event.source.path)}</span>
                    )}
                  </p>
                  <div className="mt-1.5 space-y-3">
                    {run.rows.map((row) => (
                      <EventRow key={row.event.id} row={row} />
                    ))}
                  </div>
                </li>
              ))}
            </ul>
          </section>
      ))}

      {(pagingFailed || earlier) && (
        <p aria-live="polite" className="flex flex-wrap items-center gap-x-3 pt-4 text-[0.8125rem] text-faint">
          {pagingFailed && <span className="text-ink">Earlier events could not be read: {pagingFailed}.</span>}
          {earlier && (
            <button
              type="button"
              onClick={onShowEarlier}
              disabled={loadingMore}
              className="inline-flex min-h-11 items-center text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
            >
              {loadingMore ? "Reading…" : "Show earlier"}
            </button>
          )}
        </p>
      )}
    </main>
  );
}

const UPDATED_TIME = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false });

function updatedTime(updatedAt: number): string {
  return UPDATED_TIME.format(new Date(updatedAt));
}

function differentPath(left: string, right: string): boolean {
  return left.replace(/\/+$/, "") !== right.replace(/\/+$/, "");
}
