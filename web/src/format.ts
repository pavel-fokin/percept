const DAY = new Intl.DateTimeFormat(undefined, { day: "numeric" });
const MONTH = new Intl.DateTimeFormat(undefined, { month: "long" });
const TIME = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });

/** `iso` in the browser's local time, as "9 September at 18:40" - no
 * library, `Intl.DateTimeFormat` is enough. */
export function formatDate(iso: string): string {
  const date = new Date(iso);
  return `${DAY.format(date)} ${MONTH.format(date)} at ${TIME.format(date)}`;
}

/** `n` read as "One <one>" or "N <many>" - the sketch's plural helper. */
export function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : `${n} ${many}`;
}

/** `content` cut to 57 characters plus "…" once it runs past 60 - what
 * a source's summary quotes, the sketch's inline cut ported to a
 * helper. */
export function summarize(content: string): string {
  return content.length > 60 ? `${content.slice(0, 57)}…` : content;
}

/** `error`'s message, or its string form when it isn't an `Error` - the
 * one place every `catch` reaches for one. */
export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
