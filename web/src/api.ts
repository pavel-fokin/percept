import { isRouteErrorResponse } from "react-router";

export type Session = {
  id: string;
  client_session_id: string;
  title: string | null;
  created_at: string;
  path: string;
};

export type MapSummary = {
  id: string;
  kind: string;
  path: string;
  nodes: number;
};

export type Event = {
  id: string;
  actor: "human" | "agent" | "system";
  kind: "session.created" | "session.started" | "session.stopped" | "message" | "tool.used";
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
