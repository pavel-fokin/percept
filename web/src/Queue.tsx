import { memo } from "react";
import Claim from "./Claim";
import { formatDate, plural } from "./format";
import { allClaims } from "./claims";
import type { MapQueue } from "./types";

/** One map's queue: the since line, the hint, and one `.group` per
 * settlement question - or one plain group when the map has no
 * settlement. `focused` is the id of the row a keyboard user has moved
 * to, if any. Memoised: `App` re-renders on every keystroke in the why
 * sheet, and this list does not change with it. */
function Queue({
  map,
  focused,
  onDispute,
}: {
  map: MapQueue;
  focused: string | null;
  onDispute: (id: string) => void;
}) {
  const claims = allClaims(map);

  return (
    <div>
      <div className="pt-5 pb-1">
        <p className="font-serif text-[1.0625rem]">
          {plural(claims.length, "One claim", "claims")} to {map.name}
          {map.since ? ` since you last reviewed, on ${formatDate(map.since)}.` : ". Nothing reviewed here yet."}
        </p>
        <p className="mt-1 text-[var(--ink-2)]">Mark the wrong ones.</p>
      </div>
      {map.groups.map((group) => (
        <section key={group.heading?.id ?? group.claims[0]?.id} className="mt-8">
          {group.heading && (
            <header>
              <h2 className="font-serif text-[1.1875rem] font-medium leading-snug">
                <span className="mr-2 font-sans text-sm font-normal text-[var(--ink-3)]">{group.heading.id}</span>
                {group.heading.title}
              </h2>
              <p className="mt-1 text-[var(--ink-2)]">Raised on {formatDate(group.heading.raised_at)}.</p>
            </header>
          )}
          <ol className="mt-3 list-none border-t border-[var(--rule)] p-0">
            {group.claims.map((claim) => (
              <Claim key={claim.id} claim={claim} focused={claim.id === focused} onDispute={onDispute} />
            ))}
          </ol>
        </section>
      ))}
    </div>
  );
}

export default memo(Queue);
