import { useMemo } from "react";
import { Link, LoaderFunctionArgs, useLoaderData } from "react-router";
import { getData, type Event, type Session } from "../api";
import { Markdown } from "../components/markdown";
import { RouteError } from "../components/route-error";
import { dayTime, full, time } from "../format";
import { pathName, pathUrl } from "../paths";

// One line per run of tool calls, so a tool never splits the agent's text.
type Item = { type: "message"; event: Event } | { type: "tools"; events: Event[] };

// A prompt and everything the agent did until the next one. Work before the first prompt has none.
type Exchange = { id: string; prompt?: Event; items: Item[] };

export async function loadSession({ params }: LoaderFunctionArgs) {
  const [session, events] = await Promise.all([
    getData<Session>(`/api/sessions/${params.id}`),
    getData<Event[]>(`/api/sessions/${params.id}/events`),
  ]);
  return { session, events };
}

function exchanges(events: Event[]) {
  const result: Exchange[] = [];

  for (const event of events) {
    if (event.kind === "message" && event.actor === "human") {
      result.push({ id: event.id, prompt: event, items: [] });
      continue;
    }
    if (event.kind !== "message" && event.kind !== "tool.used") continue;

    if (result.length === 0) result.push({ id: event.id, items: [] });
    const { items } = result.at(-1)!;
    const last = items.at(-1);
    if (event.kind === "message") items.push({ type: "message", event });
    else if (last?.type === "tools") last.events.push(event);
    else items.push({ type: "tools", events: [event] });
  }

  return result;
}

export function SessionPage() {
  const { session, events } = useLoaderData<typeof loadSession>();
  const list = useMemo(() => exchanges(events), [events]);
  const prompts = list.filter((e) => e.prompt).length;

  return (
    <>
      <div className="mt-8 flex flex-wrap gap-x-2 text-sm text-muted sm:mt-10">
        <nav aria-label="Breadcrumb" className="flex gap-x-2">
          <span>{pathName(session.path)}</span>
          <span aria-hidden>/</span>
          <Link to={pathUrl(session.path)} className="-my-3 inline-flex min-h-11 items-center hover:text-text">
            sessions
          </Link>
        </nav>
        <span aria-hidden>/</span>
        <h1 className="font-medium text-text">
          <time dateTime={session.created_at} title={full.format(new Date(session.created_at))}>
            {dayTime.format(new Date(session.created_at))}
          </time>
        </h1>
        <span>
          · {prompts} {prompts === 1 ? "exchange" : "exchanges"}
        </span>
      </div>

      <div className="mt-12 grid gap-16 sm:mt-14 sm:gap-20">
        {list.map((exchange) => (
          <article key={exchange.id} className="grid min-w-0 gap-6">
            {exchange.prompt && (
              <section className="min-w-0">
                <Who name="You" at={exchange.prompt.created_at} />
                <p className="leading-[26px] wrap-anywhere whitespace-pre-line">{exchange.prompt.payload.content as string}</p>
              </section>
            )}
            <AgentItems items={exchange.items} />
          </article>
        ))}
      </div>
    </>
  );
}

// The agent's name heads its first message; tool lines before it stand above.
function AgentItems({ items }: { items: Item[] }) {
  const first = items.find((item) => item.type === "message");

  return items.map((item) => {
    if (item.type === "tools") {
      return (
        <p key={item.events[0].id} className="text-sm text-muted">
          {toolRuns(item.events)}
        </p>
      );
    }

    return (
      <section key={item.event.id} className="min-w-0">
        {item === first && <Who name="Agent" at={item.event.created_at} />}
        <Markdown text={item.event.payload.content as string} />
      </section>
    );
  });
}

/** `Bash ×3 · Read · Edit ×2`: consecutive calls of one tool are counted. */
function toolRuns(events: Event[]) {
  const runs: { tool: string; count: number }[] = [];
  for (const event of events) {
    const tool = event.payload.tool as string;
    const last = runs.at(-1);
    if (last?.tool === tool) last.count += 1;
    else runs.push({ tool, count: 1 });
  }

  return runs.map(({ tool, count }) => (count > 1 ? `${tool} ×${count}` : tool)).join(" · ");
}

function Who({ name, at }: { name: string; at: string }) {
  return (
    <div className="mb-2 flex items-baseline justify-between text-[13px]">
      <span className="font-medium text-muted">{name}</span>
      <time dateTime={at} title={full.format(new Date(at))} className="text-muted tabular-nums">
        {time.format(new Date(at))}
      </time>
    </div>
  );
}

export function SessionError() {
  return <RouteError notFound="Session not found." failed="session" />;
}
