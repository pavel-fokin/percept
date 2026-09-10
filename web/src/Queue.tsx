import Claim from "./Claim";
import { formatDate, plural } from "./format";
import Mark from "./Mark";
import type { MapQueue } from "./types";

/** One map's queue: the since line, the hint, the legend, and one
 * `.group` per settlement question - or one plain group when the map
 * has no settlement. `focused` is the id of the row a keyboard user has
 * moved to, if any. */
export default function Queue({
  map,
  focused,
  onDispute,
  onConfirm,
}: {
  map: MapQueue;
  focused: string | null;
  onDispute: (id: string) => void;
  onConfirm: (id: string) => void;
}) {
  const claimed = map.groups.flatMap((group) => group.claims).filter((claim) => claim.standing === "claimed");

  return (
    <div>
      <div className="pt-5 pb-1">
        <p className="font-serif text-[1.0625rem]">
          {plural(claimed.length, "One claim", "claims")} to {map.name}
          {map.since ? ` since you last reviewed, on ${formatDate(map.since)}.` : ". Nothing reviewed here yet."}
        </p>
        <p className="mt-1 text-[var(--ink-2)]">Mark the wrong ones. Finishing marks the rest seen.</p>
        <p className="mt-2 flex items-center gap-2 text-[var(--ink-2)]">
          <Mark standing="claimed" />
          <span>A dashed ring is a claim you have not seen yet.</span>
        </p>
      </div>
      {map.groups.map((group) => {
        const isSelfGroup = group.claims.some((claim) => claim.id === group.id);
        return (
          <section key={group.id || group.claims[0]?.id} className="mt-8">
            {!isSelfGroup && (
              <header>
                <h2 className="font-serif text-[1.1875rem] font-medium leading-snug">
                  <span className="mr-2 font-sans text-sm font-normal text-[var(--ink-3)]">{group.id}</span>
                  {group.title}
                </h2>
                <p className="mt-1 text-[var(--ink-2)]">Raised on {formatDate(group.raised_at)}.</p>
              </header>
            )}
            <ol className="mt-3 list-none border-t border-[var(--rule)] p-0">
              {group.claims.map((claim) => (
                <Claim
                  key={claim.id}
                  claim={claim}
                  focused={claim.id === focused}
                  onDispute={onDispute}
                  onConfirm={onConfirm}
                />
              ))}
            </ol>
          </section>
        );
      })}
    </div>
  );
}
