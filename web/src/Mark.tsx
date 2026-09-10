import type { Standing } from "./types";

/** The mark beside a row: a dashed ring for a claim you have not seen,
 * a plain ring once seen, a filled check once confirmed, a filled bar
 * once disputed - the sketch's four-state inline SVG, ported. */
export default function Mark({ standing }: { standing: Standing }) {
  return (
    <svg className="h-[22px] w-[22px] flex-none" viewBox="0 0 22 22" aria-hidden="true">
      {standing === "claimed" && (
        <circle
          cx="11"
          cy="11"
          r="8.5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeDasharray="3.2 2.4"
        />
      )}
      {standing === "seen" && (
        <circle cx="11" cy="11" r="8.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
      )}
      {standing === "confirmed" && (
        <g>
          <circle cx="11" cy="11" r="9" fill="currentColor" />
          <path
            d="M6.5 11.5l3 3L15.5 8"
            fill="none"
            stroke="var(--ground)"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </g>
      )}
      {standing === "disputed" && (
        <g>
          <circle cx="11" cy="11" r="9" fill="currentColor" />
          <path d="M7 11h8" fill="none" stroke="var(--ground)" strokeWidth="2.2" strokeLinecap="round" />
        </g>
      )}
    </svg>
  );
}
