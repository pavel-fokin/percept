import { isRouteErrorResponse } from "react-router";

export async function getData<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Response(null, { status: res.status });
  return ((await res.json()) as { data: T }).data;
}

export function describe(error: unknown) {
  if (isRouteErrorResponse(error)) return `HTTP ${error.status}`;
  return error instanceof Error ? error.message : String(error);
}
