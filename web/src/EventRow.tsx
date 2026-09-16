import { useState } from "react";
import { fetchEvent, messageOf } from "./api";
import { answerSize, contentOf, contentText, kindWords } from "./eventRows";
import type { ContentVariant, Row } from "./eventRows";
import { Chevron } from "./icons";
import type { Event } from "./types";

export default function EventRow({ row }: { row: Row }) {
  const { event, answer } = row;
  const content = contentOf(event);
  const kind = kindWords(event);
  const wholeEvent = useWholeEvent(event, true);
  const wholeAnswer = useWholeEvent(answer, answer?.preview !== undefined);
  const shownEvent = wholeEvent.event ?? event;
  const shownAnswer = wholeAnswer.event;
  const details = payloadDetails(shownEvent);

  return (
    <details
      onToggle={(toggle) => {
        if (!toggle.currentTarget.open) return;
        wholeEvent.load();
        wholeAnswer.load();
      }}
    >
      <summary className="block min-h-11">
        <Content text={content.text} variant={content.variant} />
        <span className="mt-1.5 flex flex-wrap items-center gap-x-2 text-xs text-faint">
          {kind && (
            <span>{kind}</span>
          )}
          {kind && answer && <span className="text-rule">&#183;</span>}
          {answer && (
            <span>{answerSize(answer)}</span>
          )}
          <span data-chev className="ml-auto flex size-4 shrink-0 items-center justify-center text-faint" aria-hidden="true">
            <Chevron className="size-3.5" />
          </span>
        </span>
      </summary>
      <div className="mt-3 rounded-sm border border-rule bg-panel px-3.5 py-3">
        <FetchStatus label="Complete details" whole={wholeEvent} />
        <dl className="grid grid-cols-[5rem_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs">
          <Detail label="writer" value={shownEvent.source.name} />
          <Detail label="project path" value={shownEvent.source.path} />
          <Detail label="timestamp" value={shownEvent.created_at} />
          {event.preview !== undefined && (
            <>
              <dt className="text-faint">in full</dt>
              <dd className="break-words whitespace-pre-wrap text-[0.9375rem] leading-relaxed text-muted">
                {contentText(shownEvent).text}
              </dd>
            </>
          )}
          {answer && shownAnswer && (
            <>
              <dt className="text-faint">answer</dt>
              <dd className="font-mono break-words whitespace-pre-wrap text-[0.8125rem] leading-relaxed text-muted">
                {contentText(shownAnswer).text}
                <FetchStatus label="Full answer" whole={wholeAnswer} />
              </dd>
            </>
          )}
          {details.map(([label, value]) => (
            <Detail key={label} label={label} value={value} />
          ))}
          <dt className="text-faint">type</dt>
          <dd className="font-mono break-words text-muted">{event.type}</dd>
          <dt className="text-faint">event id</dt>
          <dd className="font-mono break-words text-muted">{event.id}</dd>
          <dt className="text-faint">caused by</dt>
          <dd className="font-mono break-words text-muted">{event.causation_id ?? "—"}</dd>
          {answer && (
            <>
              <dt className="text-faint">answer id</dt>
              <dd className="font-mono break-words text-muted">{answer.id}</dd>
            </>
          )}
        </dl>
      </div>
    </details>
  );
}

type WholeEvent = ReturnType<typeof useWholeEvent>;

function FetchStatus({ label, whole }: { label: string; whole: WholeEvent }) {
  if (whole.state === "loading") {
    return <p role="status" className="mb-3 text-xs text-faint">{label} loading…</p>;
  }
  if (whole.state === "failed") {
    return (
      <p className="mb-3 text-xs text-ink">
        {label} could not be read: {whole.message}.{" "}
        <button type="button" onClick={whole.retry} className="text-accent underline decoration-rule underline-offset-4">
          Try again
        </button>
      </p>
    );
  }
  return null;
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-faint">{label}</dt>
      <dd className="font-mono break-words whitespace-pre-wrap text-muted">{value}</dd>
    </>
  );
}

type WholeState =
  | { state: "idle" | "loading" | "ready"; event: Event | undefined }
  | { state: "failed"; event: Event | undefined; message: string };

function useWholeEvent(event: Event | undefined, shouldFetch: boolean) {
  const [whole, setWhole] = useState<WholeState>({ state: shouldFetch ? "idle" : "ready", event });

  function request() {
    if (!event || !shouldFetch) return;
    setWhole({ state: "loading", event: whole.event });
    fetchEvent(event.id)
      .then((loaded) => setWhole({ state: "ready", event: loaded }))
      .catch((error: unknown) => setWhole({ state: "failed", event: whole.event, message: messageOf(error) }));
  }

  function load() {
    if (whole.state === "idle") request();
  }

  return { ...whole, load, retry: request };
}

function payloadDetails(event: Event): [string, string][] {
  const payload = event.payload;
  switch (event.type) {
    case "tool.called":
      return details([
        ["tool", payload.tool],
        ["arguments", payload.arguments],
      ]);
    case "node.added":
    case "node.changed":
    case "node.removed":
      return details([
        ["map", payload.map],
        ["kind", payload.kind],
        ["properties", payload.properties],
        ["reason", payload.why],
        ["node", payload.node],
      ]);
    case "edge.added":
    case "edge.removed":
      return details([
        ["map", payload.map],
        ["relationship", payload.kind],
        ["from", payload.from],
        ["to", payload.to],
        ["reason", payload.why],
      ]);
    case "model.called":
      return details([
        ["model", payload.model],
        ["input tokens", payload.input_tokens],
        ["output tokens", payload.output_tokens],
        ["cached tokens", payload.cached_tokens],
      ]);
    case "file.cited":
      return details([
        ["path", payload.path],
        ["lines", payload.lines],
      ]);
    default:
      return [];
  }
}

function details(entries: [string, unknown][]): [string, string][] {
  return entries.flatMap(([label, value]) => {
    if (value === undefined || value === null || value === "") return [];
    if (typeof value === "object" && Object.keys(value).length === 0) return [];
    return [[label, typeof value === "string" ? value : JSON.stringify(value, null, 2)]];
  });
}

function Content({ text, variant }: { text: string; variant: ContentVariant }) {
  if (variant === "quote") {
    return <span className="block font-serif text-[1.0625rem] leading-relaxed">&#8220;{text}&#8221;</span>;
  }
  if (variant === "mono") {
    return <span className="block font-mono text-[0.8125rem] break-words leading-relaxed">{text}</span>;
  }
  return <span className="block text-[0.9375rem] leading-relaxed">{text}</span>;
}
