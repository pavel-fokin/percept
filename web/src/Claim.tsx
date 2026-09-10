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

/** Wrong and Confirm, on a row or on a folded option - the sketch's
 * `acts()`, ported label for label: "Wrong instead" once confirmed,
 * "Confirm instead" once disputed, Confirm hidden once confirmed. */
function Acts({
  standing,
  onDispute,
  onConfirm,
}: {
  standing: Standing;
  onDispute: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="mt-1 flex items-center gap-2">
      <button
        type="button"
        onClick={onDispute}
        className="min-h-10 rounded-md border-[1.5px] border-[var(--rule)] px-3.5 py-2 font-bold text-[var(--ink)] hover:border-[var(--object)] hover:text-[var(--object)] focus-visible:border-[var(--object)] focus-visible:text-[var(--object)] active:border-[var(--object)] active:text-[var(--object)]"
      >
        {standing === "confirmed" ? "Wrong instead" : "Wrong"}
      </button>
      {standing !== "confirmed" && (
        <button
          type="button"
          onClick={onConfirm}
          className="min-h-10 rounded-md border-[1.5px] border-transparent px-3.5 py-2 font-normal text-[var(--ink-3)] hover:text-[var(--agree)]"
        >
          {standing === "disputed" ? "Confirm instead" : "Confirm"}
        </button>
      )}
    </div>
  );
}

function OptionRow({
  option,
  onDispute,
  onConfirm,
}: {
  option: Option;
  onDispute: (id: string) => void;
  onConfirm: (id: string) => void;
}) {
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
        <Acts
          standing={option.standing}
          onDispute={() => onDispute(option.id)}
          onConfirm={() => onConfirm(option.id)}
        />
      </div>
    </li>
  );
}

/** One row of the queue: a claim's mark, headline, why, what it
 * replaced or reopens, its standing, the alternatives weighed against
 * it, and the Wrong/Confirm acts on it and on each alternative - the
 * sketch's `.claim`. `focused` renders the keyboard-navigation
 * highlight: an inset bar on a phone, an underlined id from 40rem. */
export default function Claim({
  claim,
  focused,
  onDispute,
  onConfirm,
}: {
  claim: ClaimT;
  focused: boolean;
  onDispute: (id: string) => void;
  onConfirm: (id: string) => void;
}) {
  return (
    <li
      data-id={claim.id}
      className={
        "grid grid-cols-1 gap-2 border-b border-[var(--rule)] py-4" +
        (focused ? " -ml-3 border-l-4 border-l-[var(--focus)] pl-2.5 sm:ml-0 sm:border-l-0 sm:pl-0" : "")
      }
    >
      <div className="flex items-center gap-2.5 text-[var(--ink-2)]">
        <Mark standing={claim.standing} />
        <span
          className={
            "font-bold text-[var(--ink)]" +
            (focused ? " sm:underline sm:decoration-[var(--focus)] sm:underline-offset-4" : "")
          }
        >
          {claim.id}
        </span>
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
              <OptionRow key={option.id} option={option} onDispute={onDispute} onConfirm={onConfirm} />
            ))}
          </ul>
        </details>
      )}
      <Acts
        standing={claim.standing}
        onDispute={() => onDispute(claim.id)}
        onConfirm={() => onConfirm(claim.id)}
      />
    </li>
  );
}
