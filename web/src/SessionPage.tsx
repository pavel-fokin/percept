import { useState } from "react";
import { isRouteErrorResponse, Link, LoaderFunctionArgs, useLoaderData, useRouteError } from "react-router";
import { describe, getData } from "./api";
import { absolute, time } from "./format";
import type { Session } from "./Sessions";

type Event = {
  id: string;
  actor: "human" | "agent" | "system";
  kind: "SessionCreated" | "SessionStarted" | "SessionStopped" | "Message" | "ToolUsed";
  payload: Record<string, unknown>;
  created_at: string;
};

type Block =
  | { type: "turn"; actor: "human" | "agent"; events: Event[] }
  | { type: "system"; event: Event };

export async function loadSession({ params }: LoaderFunctionArgs) {
  const [session, events] = await Promise.all([
    getData<Session>(`/api/sessions/${params.id}`),
    getData<Event[]>(`/api/sessions/${params.id}/events`),
  ]);
  return { session, events };
}

// A turn is a run of consecutive events by one actor. ToolUsed counts as the agent's.
function turns(events: Event[]) {
  const blocks: Block[] = [];

  for (const event of events) {
    if (event.kind === "SessionCreated") continue;

    if (event.kind === "SessionStarted" || event.kind === "SessionStopped") {
      blocks.push({ type: "system", event });
      continue;
    }

    const actor = event.kind === "Message" && event.actor === "human" ? "human" : "agent";
    const last = blocks.at(-1);
    if (last?.type === "turn" && last.actor === actor) last.events.push(event);
    else blocks.push({ type: "turn", actor, events: [event] });
  }

  return blocks;
}

export function SessionPage() {
  const { session, events } = useLoaderData<typeof loadSession>();
  const [open, setOpen] = useState<string>();
  const toggle = (id: string) => setOpen((current) => (current === id ? undefined : id));

  let humans = 0;

  return (
    <>
      <header className="grid gap-1 border-b border-border pt-6 pb-4">
        <Link
          to="/"
          className="-ml-2 mb-2 inline-flex min-h-8 items-center gap-1 justify-self-start rounded-md px-2 text-muted hover:bg-surface hover:text-text"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-3.5">
            <path d="M10 3 5 8l5 5" />
          </svg>
          Sessions
        </Link>
        <h2
          className={`line-clamp-3 max-w-[48ch] text-balance ${
            session.title ? "text-lg font-semibold" : "font-mono text-sm break-all text-muted"
          }`}
        >
          {session.title ?? session.key}
        </h2>
        <time dateTime={session.created_at} className="text-xs text-muted">
          {absolute(new Date(session.created_at))}
        </time>
      </header>
      <div className="grid gap-6 pt-6 pb-10">
        {turns(events).map((block) => {
          if (block.type === "system") {
            const { event } = block;
            return (
              <div
                key={event.id}
                data-open={open === event.id || undefined}
                onClick={() => toggle(event.id)}
                className="group flex cursor-default items-baseline justify-between gap-4 text-xs text-muted"
              >
                <p>{event.kind === "SessionStarted" ? "Session started" : "Session stopped"}</p>
                <Time iso={event.created_at} />
              </div>
            );
          }

          const id = block.events[0].id;
          const exchange = block.actor === "human" && humans++ > 0;

          return (
            <section
              key={id}
              data-open={open === id || undefined}
              onClick={() => toggle(id)}
              className={`group grid cursor-default gap-2 ${exchange ? "border-t border-border pt-6" : ""}`}
            >
              <div className="flex items-baseline justify-between gap-4">
                <span className="font-semibold">{block.actor === "human" ? "You" : "Agent"}</span>
                <Time iso={block.events[0].created_at} />
              </div>
              {block.events.map((event, i) => (
                <Item key={event.id} event={event} spaced={i > 0 && !(event.kind === "ToolUsed" && block.events[i - 1].kind === "ToolUsed")} />
              ))}
            </section>
          );
        })}
      </div>
    </>
  );
}

function Time({ iso }: { iso: string }) {
  return (
    <time
      dateTime={iso}
      className="invisible text-xs text-muted tabular-nums group-hover:visible group-data-open:visible"
    >
      {time.format(new Date(iso))}
    </time>
  );
}

function Item({ event, spaced }: { event: Event; spaced: boolean }) {
  const margin = spaced ? "mt-2" : "";

  if (event.kind === "Message") {
    return (
      <p className={`max-w-[68ch] text-base leading-6 break-words whitespace-pre-wrap md:text-sm ${margin}`}>
        {event.payload.content as string}
      </p>
    );
  }

  return <p className={`text-sm text-muted ${margin}`}>{event.payload.tool as string}</p>;
}

export function SessionError() {
  const error = useRouteError();

  if (isRouteErrorResponse(error) && error.status === 404) {
    return (
      <p className="pt-6">
        Session not found.{" "}
        <Link to="/" className="underline decoration-muted underline-offset-3 hover:decoration-text">
          Back to sessions
        </Link>
      </p>
    );
  }

  return <p className="pt-6">Failed to load session: {describe(error)}</p>;
}
