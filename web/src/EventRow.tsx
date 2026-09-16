import { contentOf, kindWords, projectOf, timeOf } from "./eventRows";
import { Chevron } from "./icons";
import type { Event } from "./types";

/** One event, as a `<details>` row: its content at full weight, a
 * faint details line, and the exact type, id and causation behind the
 * chevron. */
export default function EventRow({ event }: { event: Event }) {
  const content = contentOf(event);
  const kind = kindWords(event);

  return (
    <details>
      <summary className="block">
        <Content text={content.text} variant={content.variant} />
        <span className="mt-1.5 flex flex-wrap items-center gap-x-2 text-xs text-faint">
          {kind && (
            <>
              <span>{kind}</span>
              <span className="text-rule">&#183;</span>
            </>
          )}
          <span>{projectOf(event)}</span>
          <span className="text-rule">&#183;</span>
          <span className="font-mono">{timeOf(event)}</span>
          <span data-chev className="ml-auto flex size-4 shrink-0 items-center justify-center text-faint" aria-hidden="true">
            <Chevron className="size-3.5" />
          </span>
        </span>
      </summary>
      <div className="mt-3 rounded-sm border border-rule bg-panel px-3.5 py-3">
        <dl className="grid grid-cols-[5rem_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs">
          <dt className="text-faint">type</dt>
          <dd className="font-mono break-words text-muted">{event.type}</dd>
          <dt className="text-faint">id</dt>
          <dd className="font-mono break-words text-muted">{event.id}</dd>
          <dt className="text-faint">caused by</dt>
          <dd className="font-mono break-words text-muted">{event.causation_id ?? "—"}</dd>
        </dl>
      </div>
    </details>
  );
}

/** A row's full-weight line, set per `content.variant` - quoted serif
 * for a thing said, mono for what a machine emitted, plain sans for
 * everything else recorded. */
function Content({ text, variant }: { text: string; variant: "quote" | "mono" | "plain" }) {
  if (variant === "quote") {
    return <span className="block font-serif text-[1.0625rem] leading-relaxed">&#8220;{text}&#8221;</span>;
  }
  if (variant === "mono") {
    return <span className="block font-mono text-[0.8125rem] break-words leading-relaxed">{text}</span>;
  }
  return <span className="block text-[0.9375rem] leading-relaxed">{text}</span>;
}
