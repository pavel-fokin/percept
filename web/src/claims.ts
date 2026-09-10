import type { MapQueue, Row } from "./types";

/** Every row in `map`, across every group. */
export function allClaims(map: MapQueue): Row[] {
  return map.groups.flatMap((group) => group.claims);
}

/** `map`'s rows still claimed - what Finish marks seen, and what its
 * sheet counts. */
export function claimedRows(map: MapQueue): Row[] {
  return allClaims(map).filter((claim) => claim.standing === "claimed");
}
