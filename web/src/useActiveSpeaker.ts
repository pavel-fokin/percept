import { useEffect, useState } from "react";

/** How far below the top of the viewport the strip sits - a run is the
 * current one once its top has passed under here. Matches the page
 * header's height plus the strip's own. */
const STRIP_BOTTOM = 96;

/** Where each run starts, in page coordinates, and what it is - measured
 * once per layout change rather than per scroll, so a scroll costs a
 * search and no reading of the DOM. */
interface RunStart {
  top: number;
  speaker: string;
  day: string;
}

/** Who is speaking under the strip, and which day's heading may say so:
 * a page can show several days at once, and only the pinned heading is
 * looking at this run. */
export interface Speaking {
  speaker: string;
  day: string;
}

/** The speaker of the run currently under the pinned day strip - what
 * the strip names, so an attribution that has scrolled off the top is
 * still on screen. Which run sits under the strip is a fact about
 * layout, so it is read from the layout rather than tracked per row.
 *
 * `rows` is what the page is showing: the measurements are taken again
 * whenever that changes, whenever a row opens, whenever the window
 * resizes, and once the web fonts land - each of those moves everything
 * below it. */
export function useActiveSpeaker(rows: unknown): Speaking | null {
  const [speaking, setSpeaking] = useState<Speaking | null>(null);

  useEffect(() => {
    let starts: RunStart[] = [];

    function measure() {
      starts = Array.from(document.querySelectorAll<HTMLElement>("[data-speaker]")).map((run) => ({
        top: run.getBoundingClientRect().top + window.scrollY,
        speaker: run.dataset.speaker ?? "",
        day: run.dataset.day ?? "",
      }));
      pick();
    }

    /** The last run to have passed under the strip. Nothing has, when
     * the page is still at the top, and then the strip carries only the
     * date - the first run's own label is still on screen anyway. */
    function pick() {
      const line = window.scrollY + STRIP_BOTTOM;
      let found: Speaking | null = null;
      for (const start of starts) {
        if (start.top > line) break;
        found = { speaker: start.speaker, day: start.day };
      }
      // A fresh object every scroll would re-render the list at every
      // frame; React only bails out when the value is the same one.
      setSpeaking((held) => (held?.speaker === found?.speaker && held?.day === found?.day ? held : found));
    }

    measure();
    // The brand faces arrive over the network and reflow the page when
    // they land, moving every run below the fold. Without this the strip
    // names the wrong speaker for the rest of the scroll.
    document.fonts?.ready.then(measure).catch(() => {});
    // No throttle on scroll: `pick` walks a cached array and reads
    // nothing from the DOM, and React drops a set that changes nothing.
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

  return speaking;
}
