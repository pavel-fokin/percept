/** How the interface writes a number, a moment and a path. One module,
 * so two pages never word the same value differently. */

/** The interface is written in English, so its numbers and dates are
 * formatted in English too - an open locale would read `September 16`
 * on one machine and `16 September` on the next, in a row whose every
 * other word is fixed. */
export const LOCALE = "en-GB";

export function count(value: number): string {
  return value.toLocaleString(LOCALE);
}

const RELATIVE = new Intl.RelativeTimeFormat(LOCALE, { numeric: "auto" });

/** A unit and how many seconds of it make the next one up, largest
 * first - the first that `at` reaches is the one the phrase uses. */
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 60 * 60],
  ["month", 30 * 24 * 60 * 60],
  ["day", 24 * 60 * 60],
  ["hour", 60 * 60],
  ["minute", 60],
];

/** How long ago `at` was, in the coarsest unit that still says
 * something - `2 days ago`, not `47 hours ago`. Anything under a
 * minute is `just now`, since a page that says `0 minutes ago` is
 * reporting its own rounding. */
export function relativeTime(at: string, now: Date = new Date()): string {
  const seconds = (Date.parse(at) - now.getTime()) / 1000;
  const magnitude = Math.abs(seconds);
  for (const [unit, size] of UNITS) {
    if (magnitude >= size) return RELATIVE.format(Math.round(seconds / size), unit);
  }
  return "just now";
}

/** The last part of a path - the name a project is known by, where the
 * whole path is what tells two checkouts of it apart. */
export function basename(path: string): string {
  const parts = path.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] || path;
}
