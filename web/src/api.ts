import type { ReviewResponse } from "./types";

/** `GET /api/review`, throwing the status line on a failed fetch - the
 * one place the page reads the queue, so every write below refetches
 * through the same function. */
export async function fetchReview(): Promise<ReviewResponse> {
  const response = await fetch("/api/review");
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<ReviewResponse>;
}

/** Posts `body` as JSON to `path`, throwing the server's plain-text
 * reason on a refused write - `change` below shares this, so a 400 or
 * 404 reads the same way at every call site. */
async function post(path: string, body: unknown): Promise<void> {
  const response = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
}

/** `POST /api/change`: changes `node` on `map`, carrying only `why`. */
export function change(map: string, node: string, why: string): Promise<void> {
  return post("/api/change", { map, node, why });
}
