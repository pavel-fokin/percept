import { useState } from "react";
import { fetchEvent } from "./api";
import { answerSize, contentOf, contentText, kindWords, timeOf } from "./eventRows";
import type { ContentVariant, Row } from "./eventRows";
import { Chevron } from "./icons";
import type { Event } from "./types";

/** One row, as a `<details>`: its content at full weight, a faint
 * details line, and behind the chevron the exact type, id and
 * causation - plus the tool's answer, when this row is a call that
 * got one. */
export default function EventRow({ row }: { row: Row }) {
  const { event, answer } = row;
  const content = contentOf(event);
  const kind = kindWords(event);
  const answerText = useWholeContent(answer);
  // The summary shows a shortened copy of a long message; without this
  // the whole of it is reachable nowhere in the app, though the row's
  // own details line says how many characters there are.
  const wholeContent = useWholeContent(contentText(event).cut ? event : undefined);

  return (
    <details
      onToggle={(e) => {
        answerText.load(e);
        wholeContent.load(e);
      }}
    >
      <summary className="block min-h-11">
        <Content text={content.text} variant={content.variant} />
        <span className="mt-1.5 flex flex-wrap items-center gap-x-2 text-xs text-faint">
          {kind && (
            <>
              <span>{kind}</span>
              <span className="text-rule">&#183;</span>
            </>
          )}
          {answer && (
            <>
              <span>{answerSize(answer)}</span>
              <span className="text-rule">&#183;</span>
            </>
          )}
          <span className="font-mono">{timeOf(event)}</span>
          <span data-chev className="ml-auto flex size-4 shrink-0 items-center justify-center text-faint" aria-hidden="true">
            <Chevron className="size-3.5" />
          </span>
        </span>
      </summary>
      <div className="mt-3 rounded-sm border border-rule bg-panel px-3.5 py-3">
        <dl className="grid grid-cols-[5rem_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs">
          {contentText(event).cut && (
            <>
              <dt className="text-faint">in full</dt>
              <dd className="break-words whitespace-pre-wrap text-muted">{wholeContent.text}</dd>
            </>
          )}
          {answer && (
            <>
              <dt className="text-faint">answer</dt>
              <dd className="font-mono break-words whitespace-pre-wrap text-muted">{answerText.text}</dd>
            </>
          )}
          <dt className="text-faint">type</dt>
          <dd className="font-mono break-words text-muted">{event.type}</dd>
          <dt className="text-faint">id</dt>
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

/** `event`'s content, whole. The list shortens a long one to keep the
 * page's first request small, so a row that was opened asks for the
 * rest - once, and only when the list's copy was cut. A failed fetch
 * leaves the shortened text in place: a row that opens on less is
 * better than one that opens on an error. */
function useWholeContent(event: Event | undefined) {
  const shortened = event ? contentText(event) : { text: "", cut: false };
  const [whole, setWhole] = useState<string | null>(null);
  const [asked, setAsked] = useState(false);

  function load(e: React.SyntheticEvent<HTMLDetailsElement>) {
    if (!event || !shortened.cut || asked || !e.currentTarget.open) return;
    setAsked(true);
    fetchEvent(event.id)
      .then((whole) => setWhole(contentText(whole).text))
      .catch(() => {});
  }

  return { text: whole ?? shortened.text, load };
}

/** A row's full-weight line, set per `content.variant` - quoted serif
 * for a thing said, mono for what a machine emitted, plain sans for
 * everything else recorded. */
function Content({ text, variant }: { text: string; variant: ContentVariant }) {
  if (variant === "quote") {
    return <span className="block font-serif text-[1.0625rem] leading-relaxed">&#8220;{text}&#8221;</span>;
  }
  if (variant === "mono") {
    return <span className="block font-mono text-[0.8125rem] break-words leading-relaxed">{text}</span>;
  }
  return <span className="block text-[0.9375rem] leading-relaxed">{text}</span>;
}
