import type { MapQueue, Row } from "./types";

/** Every row in `map`, across every group. */
export function allClaims(map: MapQueue): Row[] {
  return map.groups.flatMap((group) => group.claims);
}
