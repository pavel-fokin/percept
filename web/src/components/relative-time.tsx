import { full, relative } from "../format";

export function RelativeTime({ at, now }: { at: string; now: Date }) {
  const date = new Date(at);

  return (
    <time dateTime={at} title={full.format(date)} className="flex-none text-xs whitespace-nowrap text-muted tabular-nums">
      {relative(date, now)}
    </time>
  );
}
