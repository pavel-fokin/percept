import { isRouteErrorResponse } from "react-router";

export type Session = {
  id: string;
  key: string;
  title: string | null;
  created_at: string;
};

export type Event = {
  id: string;
  actor: "human" | "agent" | "system";
  kind: "SessionCreated" | "SessionStarted" | "SessionStopped" | "Message" | "ToolUsed";
  payload: Record<string, unknown>;
  created_at: string;
};

export async function getData<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Response(null, { status: res.status });
  return ((await res.json()) as { data: T }).data;
}

export function describe(error: unknown) {
  if (isRouteErrorResponse(error)) return `HTTP ${error.status}`;
  return error instanceof Error ? error.message : String(error);
}
