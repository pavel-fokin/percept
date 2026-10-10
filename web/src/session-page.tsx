import { useMemo, useState } from "react";
import { isRouteErrorResponse, Link, LoaderFunctionArgs, useLoaderData, useRouteError } from "react-router";
import { describe, getData, type Event, type Session } from "./api";
import { full, time } from "./format";

// `divided` marks a new exchange: a human turn after the first one.
type Block =
  | { type: "turn"; actor: "human" | "agent"; events: Event[]; divided: boolean }
  | { type: "system"; event: Event };

export async function loadSession({ params }: LoaderFunctionArgs) {
  const [session, events] = await Promise.all([
    getData<Session>(`/api/sessions/${params.id}`),
    getData<Event[]>(`/api/sessions/${params.id}/events`),
  ]);
  return { session, events };
}

// A turn is a run of consecutive events by one actor; system events stand alone.
function turns(events: Event[]) {
  const blocks: Block[] = [];
  let humanTurns = 0;

  for (const event of events) {
    if (event.kind === "SessionCreated") continue;

    if (event.actor === "system") {
      blocks.push({ type: "system", event });
      continue;
    }

    const { actor } = event;
    const last = blocks.at(-1);
    if (last?.type === "turn" && last.actor === actor) last.events.push(event);
    else blocks.push({ type: "turn", actor, events: [event], divided: actor === "human" && humanTurns++ > 0 });
  }

  return blocks;
}

export function SessionPage() {
  const { session, events } = useLoaderData<typeof loadSession>();
  const blocks = useMemo(() => turns(events), [events]);
  const [open, setOpen] = useState<string>();
  // A click that ends a text selection is for copying, not for the time.
  const toggle = (id: string) => {
    if (getSelection()?.toString()) return;
    setOpen((current) => (current === id ? undefined : id));
  };

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
          {full.format(new Date(session.created_at))}
        </time>
      </header>
      <div className="grid gap-6 pt-6 pb-10">
        {blocks.map((block) => {
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

          return (
            <section
              key={id}
              data-open={open === id || undefined}
              onClick={() => toggle(id)}
              className={`group grid cursor-default gap-2 ${block.divided ? "border-t border-border pt-6" : ""}`}
            >
              <div className="flex items-baseline justify-between gap-4">
                <span className="font-semibold">{block.actor === "human" ? "You" : "Agent"}</span>
                <Time iso={block.events[0].created_at} />
              </div>
              {block.events.map((event, i) => (
                <Item key={event.id} event={event} previous={block.events[i - 1]} />
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

// Tool lines stack tight; a message keeps a gap from whatever comes before or after it.
function Item({ event, previous }: { event: Event; previous?: Event }) {
  const stacked = event.kind === "ToolUsed" && previous?.kind === "ToolUsed";
  const margin = previous && !stacked ? "mt-2" : "";

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

  if (isRouteErrorResponse(error) && (error.status === 404 || error.status === 400)) {
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
