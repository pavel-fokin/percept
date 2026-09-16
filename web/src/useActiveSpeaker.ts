import { useEffect, useState } from "react";

/** How far below the top of the viewport the strip sits - a run is the
 * current one once its top has passed under here. Matches the page
 * header's height plus the strip's own. */
const STRIP_BOTTOM = 96;

/** Where each run starts, in page coordinates, and who is speaking in
 * it. Measured once per layout change rather than per scroll, so a
 * scroll costs a search and no reading of the DOM. */
interface RunStart {
  top: number;
  speaker: string;
}

/** The speaker of the run currently under the pinned day strip - what
 * the strip names, so an attribution that has scrolled off the top is
 * still on screen. Which run sits under the strip is a fact about
 * layout, so it is read from the layout rather than tracked per row.
 *
 * `rows` is what the page is showing: the measurements are taken again
 * whenever that changes, and whenever a row opens or the window
 * resizes, since each of those moves everything below it. */
export function useActiveSpeaker(rows: unknown): string | null {
  const [speaker, setSpeaker] = useState<string | null>(null);

  useEffect(() => {
    let starts: RunStart[] = [];

    function measure() {
      starts = Array.from(document.querySelectorAll<HTMLElement>("[data-speaker]")).map((run) => ({
        top: run.getBoundingClientRect().top + window.scrollY,
        speaker: run.dataset.speaker ?? "",
      }));
      pick();
    }

    /** The last run to have passed under the strip. Nothing has, when
     * the page is still at the top, and then the strip carries only the
     * date - the first run's own label is still on screen anyway. */
    function pick() {
      const line = window.scrollY + STRIP_BOTTOM;
      let found: string | null = null;
      for (const start of starts) {
        if (start.top > line) break;
        found = start.speaker;
      }
      setSpeaker(found);
    }

    measure();
    // No throttle: `pick` walks a cached array and reads nothing from
    // the DOM, and React drops a set that changes nothing, so a scroll
    // costs a comparison per run and usually no render at all.
    window.addEventListener("scroll", pick, { passive: true });
    window.addEventListener("resize", measure);
    // `toggle` does not bubble, so a row opening is caught on the way down.
    document.addEventListener("toggle", measure, true);
    return () => {
      window.removeEventListener("scroll", pick);
      window.removeEventListener("resize", measure);
      document.removeEventListener("toggle", measure, true);
    };
  }, [rows]);

  return speaker;
}
