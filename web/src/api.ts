import { isRouteErrorResponse } from "react-router";

export type Project = {
  name: string;
  root: string | null;
};

export type Session = {
  id: string;
  client_session_id: string;
  title: string | null;
  created_at: string;
  last_event_at: string;
  status: "working" | "waiting" | "ended";
  project: string;
  client: string;
};

export type MapSummary = {
  name: string;
  project: string;
  nodes: number;
};

export type BoardSummary = {
  id: string;
  name: string;
  project: string;
};

export type MapNode = {
  id: string;
  kind: string;
  title: string;
  properties: Record<string, string>;
  references: Reference[];
  created_at: string;
  changed_at: string | null;
};

export type Reference =
  | { message: { event: string; session: string; created_at: string; quote: string } }
  | { file: { path: string; lines: [number, number]; excerpt: string } };

export type MapEdge = {
  kind: string;
  from: string;
  to: string;
};

export type MapDetail = {
  name: string;
  project: string;
  nodes: MapNode[];
  edges: MapEdge[];
};

export type Event = {
  id: string;
  actor: "human" | "agent" | "system";
  kind:
    | "session.created"
    | "session.started"
    | "session.stopped"
    | "message"
    | "tool.used"
    | "subagent.started"
    | "subagent.reported";
  payload: Record<string, unknown>;
  created_at: string;
};

export async function getData<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Response(null, { status: res.status });
  return ((await res.json()) as { data: T }).data;
}

/** Posts `body` as JSON to `url`, or nothing; a refusal throws an Error with the server's reason. */
export async function postData<T>(url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    ...(body === undefined ? {} : { headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }),
  });
  if (!res.ok) throw new Error((await res.text()) || `HTTP ${res.status}`);
  return ((await res.json()) as { data: T }).data;
}

export function describe(error: unknown) {
  if (isRouteErrorResponse(error)) return `HTTP ${error.status}`;
  return error instanceof Error ? error.message : String(error);
}
