import { Link } from "react-router";
import type { Session } from "../api";
import { full, relative } from "../format";

export function SessionRow({ session, now }: { session: Session; now: Date }) {
  const started = new Date(session.created_at);

  return (
    <li className="border-t border-border first:border-t-0 has-[>a:hover]:border-transparent [li:has(>a:hover)+&]:border-transparent">
      <Link
        to={`/sessions/${session.id}`}
        className="-mx-3 flex min-h-11 flex-col gap-0.5 rounded-md p-3 hover:bg-surface focus-visible:bg-surface md:flex-row md:items-baseline md:gap-6"
      >
        <span
          className={`min-w-0 flex-1 line-clamp-2 text-base md:line-clamp-1 md:text-sm ${
            session.title ? "" : "font-mono text-muted"
          }`}
        >
          {session.title ?? session.client_session_id}
        </span>
        <time
          dateTime={session.created_at}
          title={full.format(started)}
          className="flex-none text-xs whitespace-nowrap text-muted tabular-nums"
        >
          {relative(started, now)}
        </time>
      </Link>
    </li>
  );
}
