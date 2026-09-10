/** `iso` in the browser's local time, as "9 September at 18:40" - no
 * library, `Intl.DateTimeFormat` is enough. */
export function formatDate(iso: string): string {
  const date = new Date(iso);
  const day = new Intl.DateTimeFormat(undefined, { day: "numeric" }).format(date);
  const month = new Intl.DateTimeFormat(undefined, { month: "long" }).format(date);
  const time = new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(date);
  return `${day} ${month} at ${time}`;
}

/** `n` read as "One <one>" or "N <many>" - the sketch's plural helper. */
export function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : `${n} ${many}`;
}
