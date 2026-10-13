import type { ReactNode } from "react";
import { Link } from "react-router";

/** An arrow up a level, then the collection's path; children follow it on the same line. */
export function BackLink({ to, path, collection, children }: { to: string; path: string; collection: string; children?: ReactNode }) {
  return (
    <span className="block pl-6 -indent-6 font-mono text-[13px] wrap-anywhere text-muted">
      <Link to={to} className="py-3 hover:text-text">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="mr-2 inline size-4 align-[-3px]">
          <path d="M13 8H3m5-5L3 8l5 5" />
        </svg>
        {`${path}/${collection}`}
      </Link>
      {children}
    </span>
  );
}
