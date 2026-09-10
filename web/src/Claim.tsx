import Mark from "./Mark";
import { plural } from "./format";
import type { Claim as ClaimT, Option, Standing } from "./types";

const standingText: Partial<Record<Standing, string>> = {
  seen: "Seen by you.",
  confirmed: "Confirmed by you.",
  disputed: "Marked wrong by you.",
};

function StandingText({ standing }: { standing: Standing }) {
  const text = standingText[standing];
  if (!text) return null;
  const color =
    standing === "confirmed"
      ? "text-[var(--agree)]"
      : standing === "disputed"
        ? "text-[var(--object)]"
        : "text-[var(--ink-3)]";
  return <p className={color}>{text}</p>;
}

function Dispute({ dispute }: { dispute: string | null }) {
  if (!dispute) return null;
  return (
    <div className="mt-1 border-l-2 border-[var(--object)] pl-4 font-serif">
      <p className="text-[var(--ink-2)]">Why you marked it wrong.</p>
      <p>{dispute}</p>
    </div>
  );
}

function OptionRow({ option }: { option: Option }) {
  return (
    <li className="grid grid-cols-[auto_1fr] items-start gap-2.5 py-2.5">
      <div className="flex flex-col items-start gap-1 text-[var(--ink-2)]">
        <Mark standing={option.standing} />
        <span className="text-[var(--ink)]">{option.id}</span>
      </div>
      <div>
        <p className="font-serif">
          <i>weighed and lost,</i> {option.name}
        </p>
        {option.why && (
          <p className="font-serif text-[var(--ink-2)]">
            <i>because</i> {option.why}
          </p>
        )}
        <StandingText standing={option.standing} />
        <Dispute dispute={option.dispute} />
      </div>
    </li>
  );
}

/** One row of the queue: a claim's mark, headline, why, what it
 * replaced or reopens, its standing, and the alternatives weighed
 * against it, if any - the sketch's `.claim`, read-only. */
export default function Claim({ claim }: { claim: ClaimT }) {
  return (
    <li className="grid grid-cols-1 gap-2 border-b border-[var(--rule)] py-4">
      <div className="flex items-center gap-2.5 text-[var(--ink-2)]">
        <Mark standing={claim.standing} />
        <span className="font-bold text-[var(--ink)]">{claim.id}</span>
        <span>{claim.kind}</span>
      </div>
      <p className="font-serif text-[1.0625rem] leading-relaxed">{claim.name}</p>
      {claim.why && (
        <p className="font-serif text-[var(--ink-2)]">
          <i>because</i> {claim.why}
        </p>
      )}
      {claim.was && (
        <p className="text-[var(--ink-2)]">
          Replaced {claim.was.id}, {claim.was.name}.
        </p>
      )}
      {claim.reopens.map((decision) => (
        <p key={decision.id} className="text-[var(--focus)]">
          It reopens {decision.id}, {decision.name}, which stands until you settle this.
        </p>
      ))}
      <StandingText standing={claim.standing} />
      <Dispute dispute={claim.dispute} />
      {claim.options.length > 0 && (
        <details className="group">
          <summary className="flex min-h-[44px] cursor-pointer list-none items-center text-[var(--ink-2)]">
            <span aria-hidden="true" className="mr-2 inline-block transition-transform group-open:rotate-90">
              ▸
            </span>
            {plural(claim.options.length, "One alternative", "alternatives")} weighed and lost
          </summary>
          <ul className="ml-4 mt-2 list-none border-l-2 border-[var(--rule)] pl-4">
            {claim.options.map((option) => (
              <OptionRow key={option.id} option={option} />
            ))}
          </ul>
        </details>
      )}
    </li>
  );
}
