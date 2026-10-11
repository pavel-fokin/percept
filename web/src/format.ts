export const time = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });
const day = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" });
export const full = new Intl.DateTimeFormat("en-GB", { dateStyle: "medium", timeStyle: "medium" });

export function relative(date: Date, now: Date) {
  const minutes = Math.floor((now.getTime() - date.getTime()) / 60_000);
  if (minutes < 60) return `${Math.max(minutes, 0)} min ago`;
  if (minutes < 360) return `${Math.floor(minutes / 60)} h ago`;

  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (date >= midnight) return `Today, ${time.format(date)}`;

  const yesterday = new Date(midnight);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date >= yesterday) return `Yesterday, ${time.format(date)}`;

  return day.format(date);
}
