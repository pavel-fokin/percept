export const time = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });
const day = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" });
export const full = new Intl.DateTimeFormat("en-GB", { dateStyle: "medium", timeStyle: "medium" });
export const dayTime = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

export function relative(date: Date, now: Date) {
  const minutes = Math.floor((now.getTime() - date.getTime()) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 360) return `${Math.floor(minutes / 60)} h ago`;

  const name = dayName(date, now);
  return name === "Today" || name === "Yesterday" ? `${name}, ${time.format(date)}` : name;
}

/** The day a date falls on, as a heading: `Today`, `Yesterday` or `11 Oct`. */
export function dayName(date: Date, now: Date) {
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (date >= midnight) return "Today";

  const yesterday = new Date(midnight);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date >= yesterday) return "Yesterday";

  return day.format(date);
}

/** A date as its day heading and clock time: `Today, 16:05` or `11 Oct, 09:30`. */
export function dayAndTime(date: Date, now: Date) {
  return `${dayName(date, now)}, ${time.format(date)}`;
}

/** When a row's item changed, in a section headed `section`: the clock time on that day, else the day too, `yesterday, 16:05`. */
export function changedWhen(date: Date, section: string, now: Date) {
  const name = dayName(date, now);
  if (name === section) return time.format(date);

  const day = name === "Today" || name === "Yesterday" ? name.toLowerCase() : name;
  return `${day}, ${time.format(date)}`;
}

/** Items grouped by the day they fall on. Items come newest first, so a day's items are consecutive. */
export function byDay<T>(items: T[], at: (item: T) => string, now: Date) {
  const days: { name: string; items: T[] }[] = [];

  for (const item of items) {
    const name = dayName(new Date(at(item)), now);
    const last = days.at(-1);
    if (last?.name === name) last.items.push(item);
    else days.push({ name, items: [item] });
  }

  return days;
}

/** Each item with its clock time, left out when the item before it shows the same minute. */
export function withChangedTimes<T>(items: T[], at: (item: T) => string) {
  return items.map((item, i) => {
    const shown = time.format(new Date(at(item)));
    const same = i > 0 && time.format(new Date(at(items[i - 1]))) === shown;
    return { item, shown: same ? undefined : shown };
  });
}
