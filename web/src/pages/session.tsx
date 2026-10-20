import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useLocation, useNavigationType } from "react-router";
import { getData, type Event, type Session } from "../api";
import { BackLink } from "../components/back-link";
import { Chevron } from "../components/chevron";
import { Markdown } from "../components/markdown";
import { RouteError } from "../components/route-error";
import { Spinner } from "../components/spinner";
import { dayTime, full, time } from "../format";
import { sessionsUrl } from "../paths";
import { usePolling } from "../use-polling";

// One line per run of tool calls, so a tool never splits the agent's text.
type Item = { type: "event"; event: Event } | { type: "tools"; events: Event[] };

// What a subagent was sent to do, by the client's id for it.
type Subagents = Map<string, { agent: string; task: string }>;

// A prompt and everything the agent did until the next one. Work before the first prompt has none.
type Exchange = { id: string; prompt?: Event; items: Item[] };

export async function loadSession(path: string, id: string) {
  const [session, events] = await Promise.all([
    getData<Session>(`/api/sessions/${id}`),
    getData<Event[]>(`/api/sessions/${id}/events`),
  ]);
  if (session.path !== path) throw new Response(null, { status: 404 });

  return { session, events };
}

function exchanges(events: Event[]) {
  const result: Exchange[] = [];

  for (const event of events) {
    if (event.kind === "message" && event.actor === "human") {
      result.push({ id: event.id, prompt: event, items: [] });
      continue;
    }
    if (!["message", "tool.used", "subagent.started", "subagent.reported"].includes(event.kind)) continue;

    if (result.length === 0) result.push({ id: event.id, items: [] });
    const { items } = result.at(-1)!;
    const last = items.at(-1);
    if (event.kind !== "tool.used") items.push({ type: "event", event });
    else if (last?.type === "tools") last.events.push(event);
    else items.push({ type: "tools", events: [event] });
  }

  return result;
}

function subagents(events: Event[]): Subagents {
  const result: Subagents = new Map();
  for (const event of events) {
    if (event.kind !== "subagent.started") continue;
    const { subagent, agent, task } = event.payload as { subagent: string; agent: string; task: string };
    result.set(subagent, { agent, task });
  }

  return result;
}

export function SessionPage({ session, events }: { session: Session; events: Event[] }) {
  const list = useMemo(() => exchanges(events), [events]);
  const started = useMemo(() => subagents(events), [events]);
  const prompted = list.filter((e) => e.prompt);
  const prompts = prompted.length;
  usePolling(5000, session.status !== "ended");
  useQuoteHighlight();
  useScrollToLastPrompt(prompted.at(-1)?.id);

  return (
    <>
      <nav aria-label="Breadcrumb" className="mt-8 sm:mt-10">
        <BackLink to={sessionsUrl(session.path)} path={session.path} collection="sessions" />
      </nav>
      <h1 className="mt-1.5 text-sm font-normal text-muted">
        started{" "}
        <time dateTime={session.created_at} title={full.format(new Date(session.created_at))} className="text-text">
          {dayTime.format(new Date(session.created_at))}
        </time>{" "}
        · {prompts} {prompts === 1 ? "exchange" : "exchanges"} · {clientName(session.client)}
      </h1>

      <div className="mt-12 grid gap-16 sm:mt-14 sm:gap-20">
        {list.map((exchange) => (
          <article key={exchange.id} className="grid min-w-0 gap-6">
            {exchange.prompt && (
              <section id={exchange.prompt.id} className={messageClass}>
                <Who name="You" at={exchange.prompt.created_at} />
                <p className="leading-[26px] wrap-anywhere whitespace-pre-line">{exchange.prompt.payload.content as string}</p>
              </section>
            )}
            <AgentItems items={exchange.items} started={started} agent={clientName(session.client)} />
            {exchange === list.at(-1) && session.status === "working" && (
              <p className="flex items-center gap-1.5 text-sm text-muted">
                <Spinner />
                {clientName(session.client)} is working
              </p>
            )}
          </article>
        ))}
      </div>
    </>
  );
}

const clientNames: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex" };

function clientName(client: string) {
  return clientNames[client] ?? client;
}

// The agent's name heads its first message; quiet lines before it stand above.
function AgentItems({ items, started, agent }: { items: Item[]; started: Subagents; agent: string }) {
  const first = items.find((item) => item.type === "event" && isAgentMessage(item.event));

  return items.map((item) => {
    if (item.type === "tools") return <Tools key={item.events[0].id} events={item.events} />;

    const { event } = item;
    const payload = event.payload as Record<string, string>;

    if (event.kind === "subagent.started") return <Quiet key={event.id} id={event.id}>{payload.agent} · {payload.task}</Quiet>;
    if (event.kind === "subagent.reported") return <Report key={event.id} content={payload.content} launch={started.get(payload.subagent)} />;
    if (!isAgentMessage(event)) return <Quiet key={event.id} id={event.id}>{payload.content}</Quiet>;

    return (
      <section key={event.id} id={event.id} className={messageClass}>
        {item === first && <Who name={agent} at={event.created_at} />}
        <Markdown text={payload.content} />
      </section>
    );
  });
}

function isAgentMessage(event: Event) {
  return event.kind === "message" && event.actor === "agent";
}

// A reference opens a message by its event id, so the message clears the top edge it scrolls to.
const messageClass = "min-w-0 scroll-mt-6";

function Quiet({ id, children }: { id: string; children: ReactNode }) {
  return <p id={id} className={`${messageClass} text-sm text-muted wrap-anywhere`}>{children}</p>;
}

/** Opens a session at its last prompt, so the reader starts with the latest question and its answer. Once per page open: a revalidation that brings a newer prompt does not move the reader. */
function useScrollToLastPrompt(id: string | undefined) {
  const { hash, key } = useLocation();
  const navigation = useNavigationType();

  useEffect(() => {
    // Back, forward and reload return to where the reader was; only a fresh tab's first entry has the default key.
    if (hash || !id || (navigation === "POP" && key !== "default")) return;
    document.getElementById(id)?.scrollIntoView();
  }, [hash, key, navigation]);
}

/** Marks the `?q=` quote inside the message `#<event>` names. A quote that spans markdown syntax is not in the rendered text and stays unmarked. */
function useQuoteHighlight() {
  const { hash, search } = useLocation();

  useEffect(() => {
    const quote = new URLSearchParams(search).get("q");
    const message = document.getElementById(decodeURIComponent(hash.slice(1)));
    if (!quote || !message) return;

    const range = rangeOf(message, quote);
    if (!range) return;

    CSS.highlights.set("quote", new Highlight(range));
    return () => void CSS.highlights.delete("quote");
  }, [hash, search]);
}

// The rendered text splits across text nodes, so the quote is found in their joined text and mapped back.
function rangeOf(root: Element, quote: string) {
  const nodes: Text[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);

  const at = nodes.map((node) => node.data).join("").indexOf(quote);
  if (at < 0) return null;

  const range = document.createRange();
  let offset = 0;
  for (const node of nodes) {
    const end = offset + node.length;
    if (at >= offset && at < end) range.setStart(node, at - offset);
    if (at + quote.length > offset && at + quote.length <= end) range.setEnd(node, at + quote.length - offset);
    offset = end;
  }

  return range;
}

const summaryClass =
  "-mx-2.5 flex min-h-11 cursor-pointer list-none items-center gap-1.5 rounded-lg px-2.5 text-sm text-muted hover:bg-surface [&::-webkit-details-marker]:hidden";

function Report({ content, launch }: { content: string; launch?: { agent: string; task: string } }) {
  return (
    <details className="group min-w-0">
      <summary className={summaryClass}>
        <Chevron />
        <span className="min-w-0 wrap-anywhere">
          {launch ? `${launch.agent} reported · ${launch.task}` : "subagent reported"}
        </span>
      </summary>
      <div className="pt-2">
        <Markdown text={content} />
      </div>
    </details>
  );
}

function Tools({ events }: { events: Event[] }) {
  return (
    <details className="group min-w-0">
      <summary className={summaryClass}>
        <Chevron />
        <span className="min-w-0 wrap-anywhere">{toolRuns(events)}</span>
      </summary>
      <div className="grid min-w-0 pb-2 pl-5">
        {events.map((event) => (
          <ToolCall key={event.id} payload={event.payload as ToolPayload} />
        ))}
      </div>
    </details>
  );
}

type ToolPayload = { tool: string; input: string | null; output: string | null };

// A call opens on its own to its full input and output; one with neither is a plain line, indented to match.
// The body mounts on first open: a long session holds hundreds of outputs nobody reads.
function ToolCall({ payload: { tool, input, output } }: { payload: ToolPayload }) {
  const [opened, setOpened] = useState(false);

  const line = (
    <span className="flex min-w-0 gap-2 font-mono text-[13px]">
      <span className="shrink-0 text-text">{tool}</span>
      {input && (
        <span title={input} className="min-w-0 truncate">
          {input}
        </span>
      )}
    </span>
  );

  if (!input && !output) return <div className="flex min-h-11 items-center pl-5 text-muted sm:min-h-9">{line}</div>;

  return (
    <details className="group/call min-w-0" onToggle={(e) => e.currentTarget.open && setOpened(true)}>
      <summary className={`${summaryClass} sm:min-h-9`}>
        <Chevron open="group-open/call:rotate-90" />
        {line}
      </summary>
      {opened && (
        <div className="mt-1 mb-2 grid gap-2 font-mono text-[13px] leading-[20px]">
          {input && <p className="pl-5 whitespace-pre-wrap wrap-anywhere">{input}</p>}
          {output && <pre className="max-h-80 overflow-auto rounded-[10px] bg-surface px-4 py-3.5">{output}</pre>}
        </div>
      )}
    </details>
  );
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
