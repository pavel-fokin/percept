import { useState } from "react";
import EventRow from "./EventRow";
import { eventsCountLabel, foldResults, groupByDay, groupRuns } from "./eventRows";
import FilterMenu from "./FilterMenu";
import type { Filter } from "./filters";
import { KIND_OPTIONS, TIME_OPTIONS, isFilterActive, kindsLabel, timeLabel, toggleKind } from "./filters";
import { SearchIcon } from "./icons";
import { useActiveSpeaker } from "./useActiveSpeaker";
import type { Event } from "./types";

/** Which of the two filter menus is open - never both, so opening one
 * closes the other for free. */
type OpenMenu = "kind" | "time" | null;

/** The screen's body: search and filters over the log, grouped by day
 * and, within a day, by the run of consecutive rows one speaker
 * produced. A tool's answer is not a row of its own - it sits inside
 * the call that caused it. */
export default function Log({
  events,
  total,
  loadingMore,
  onShowEarlier,
  filter,
  onFilterChange,
  onClear,
}: {
  events: Event[];
  total: number;
  loadingMore: boolean;
  onShowEarlier: () => void;
  filter: Filter;
  onFilterChange: (filter: Filter) => void;
  onClear: () => void;
}) {
  const [openMenu, setOpenMenu] = useState<OpenMenu>(null);
  const days = groupByDay(foldResults(events));
  const speaking = useActiveSpeaker(events);
  const earlier = events.length < total;
  const filtered = isFilterActive(filter);
  const noMatches = filtered && total === 0;

  return (
    <main id="log" className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <label htmlFor="q" className="sr-only">
        Search every event
      </label>
      <div className="relative mt-4">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-0 size-4 -translate-y-1/2 text-faint" />
        <input
          id="q"
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
            onClick={onClear}
            className="inline-flex min-h-11 items-center text-[0.8125rem] text-accent underline decoration-rule underline-offset-4 hover:decoration-faint"
          >
            Clear
          </button>
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
            {speaking && (
              <>
                <span className="text-rule">&#183;</span>
                <span className="text-faint">{speaking}</span>
              </>
            )}
          </h2>
            <ul className="mt-2">
              {groupRuns(day.rows).map((run, runIndex, runs) => (
                <li
                  key={runIndex}
                  data-speaker={run.speaker.label}
                  className={"border-t border-rule py-5" + (runIndex === runs.length - 1 ? " border-b" : "")}
                >
                  <p className={`text-[0.6875rem] font-medium uppercase tracking-[0.1em] ${run.speaker.toneClass}`}>
                    {run.speaker.label}
                  </p>
                  <div className={run.rows.length > 1 ? "mt-2 space-y-5" : "mt-2"}>
                    {run.rows.map((row) => (
                      <EventRow key={row.event.id} row={row} />
                    ))}
                  </div>
                </li>
              ))}
            </ul>
          </section>
      ))}

      <p aria-live="polite" className="flex flex-wrap items-center gap-x-3 pt-4 text-[0.8125rem] text-faint">
        <span className={noMatches ? "mt-4 text-[0.9375rem] text-muted" : undefined}>
          {noMatches ? "Nothing in the log matches that filter." : eventsCountLabel(events.length, total)}
        </span>
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
    </main>
  );
}
