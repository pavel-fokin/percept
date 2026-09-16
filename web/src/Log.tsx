import EventRow from "./EventRow";
import { foldResults, groupByDay, groupRuns } from "./eventRows";
import { Chevron, SearchIcon } from "./icons";
import type { Event } from "./types";

/** The screen's body: the (inert) search and filters, then the log
 * grouped by day and, within a day, by the run of consecutive rows one
 * speaker produced. A tool's answer is not a row of its own - it sits
 * inside the call that caused it. */
export default function Log({ events, total }: { events: Event[]; total: number }) {
  const days = groupByDay(foldResults(events));

  return (
    <main className="mx-auto w-full max-w-3xl flex-1 px-4 pb-14 sm:px-8">
      <label htmlFor="q" className="sr-only">
        Search every event
      </label>
      <div className="relative mt-4">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-0 size-4 -translate-y-1/2 text-faint" />
        <input
          id="q"
          type="search"
          placeholder="Search every event&#8230;"
          className="min-h-11 w-full border-0 border-b border-rule bg-transparent pr-0 pl-7 text-[0.9375rem] placeholder:text-faint focus:border-accent focus:outline-none"
        />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <button
          type="button"
          className="inline-flex min-h-11 items-center gap-1.5 rounded-sm border border-rule px-3 text-[0.8125rem] text-muted hover:text-ink"
        >
          <span>All kinds</span>
          <Chevron className="size-3 text-faint" />
        </button>
        <button
          type="button"
          className="inline-flex min-h-11 items-center gap-1.5 rounded-sm border border-rule px-3 text-[0.8125rem] text-muted hover:text-ink"
        >
          <span>All time</span>
          <Chevron className="size-3 text-faint" />
        </button>
      </div>

      {days.map((day, i) => (
        <section key={day.key}>
          <h2 className={`${i === 0 ? "mt-7" : "mt-8"} text-xs font-medium uppercase tracking-[0.09em] text-muted`}>{day.label}</h2>
          <ul className="mt-2">
            {groupRuns(day.rows).map((run, runIndex, runs) => (
              <li
                key={runIndex}
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

      <p className="pt-4 text-[0.8125rem] text-faint">
        {events.length} of {total} events, newest first.
      </p>
    </main>
  );
}
