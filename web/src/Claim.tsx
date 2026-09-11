import { formatDate, plural, summarize } from "./format";
import type { OptionRow as OptionRowT, Row as RowT, Source as SourceT } from "./types";

/** One quiet line naming who last changed a row, and why, when its
 * last change is not its addition. `addedAt` is the row's `added_at`,
 * given only for a claim - an alternative carries none, so it shows
 * the line whenever a `changed_why` says there was one. */
function ChangedBy({ base, addedAt }: { base: OptionRowT; addedAt?: string }) {
  const changed = addedAt !== undefined ? base.changed_at !== addedAt : Boolean(base.changed_why);
  if (!changed) return null;
  return (
    <p className="text-[var(--ink-3)]">
      changed by {base.changed_by}
      {base.changed_why ? `: ${base.changed_why}` : ""}
    </p>
  );
}

/** The line a truncated source's exchange ends with, so a reader who
 * needs the rest knows where to take it. */
function TruncatedNote({ id }: { id: string }) {
  return (
    <p className="mt-1 text-[var(--ink-2)]">
      Cut here; the whole event is <code>percept events show {id}</code>.
    </p>
  );
}

/** A fold shared by a row's sources and its alternatives: a 44px
 * chevron summary that rotates open with its `<details>`. `source`
 * marks the three source folds, so `App`'s `s` shortcut can find one
 * with `details.source`. */
function Fold({
  summary,
  children,
  source,
}: {
  summary: React.ReactNode;
  children: React.ReactNode;
  source?: boolean;
}) {
  return (
    <details className={source ? "group source" : "group"}>
      <summary className="flex min-h-[44px] cursor-pointer list-none items-center text-[var(--ink-2)]">
        <span aria-hidden="true" className="mr-2 inline-block transition-transform group-open:rotate-90">
          ▸
        </span>
        {summary}
      </summary>
      {children}
    </details>
  );
}

/** A `message` source: a human prompt folds open to the exchange grid
 * - the proposal it answered, if any, over the prompt itself; an agent
 * reply folds open to the reply text alone. */
function MessageSource({ source }: { source: Extract<SourceT, { kind: "message" }> }) {
  if (source.actor === "agent") {
    return (
      <Fold source summary={<>From the model&rsquo;s reply at {formatDate(source.at)}. Show it.</>}>
        <div className="mt-2 whitespace-pre-wrap border-l-2 border-[var(--rule)] pl-4 font-serif">
          <p>{source.content}</p>
          {source.truncated && <TruncatedNote id={source.id} />}
        </div>
      </Fold>
    );
  }
  return (
    <Fold
      source
      summary={
        <>
          You said &ldquo;{summarize(source.content)}&rdquo; at {formatDate(source.at)}. Show the exchange.
        </>
      }
    >
      <div className="mt-2 grid grid-cols-[3.5rem_1fr] gap-x-2 gap-y-3 border-l-2 border-[var(--rule)] pl-4 font-serif">
        {source.proposal && (
          <>
            <span className="font-sans text-sm leading-tight text-[var(--ink-3)]">
              model
              <br />
              {formatDate(source.proposal.at)}
            </span>
            <p className="whitespace-pre-wrap">{source.proposal.content}</p>
          </>
        )}
        <span className="font-sans text-sm leading-tight text-[var(--ink-3)]">
          you
          <br />
          {formatDate(source.at)}
        </span>
        <p className="whitespace-pre-wrap">{source.content}</p>
      </div>
      {source.truncated && <TruncatedNote id={source.id} />}
    </Fold>
  );
}

/** A `file` source: folds open to its excerpt, in a `<pre>` that
 * scrolls on its own rather than widening the row. */
function FileSource({ source }: { source: Extract<SourceT, { kind: "file" }> }) {
  return (
    <Fold source summary={<>Cites {source.label}.</>}>
      <div className="mt-2 overflow-x-auto border-l-2 border-[var(--rule)] pl-4">
        <pre className="whitespace-pre font-mono text-sm">{source.excerpt}</pre>
      </div>
      {source.truncated && <TruncatedNote id={source.id} />}
    </Fold>
  );
}

/** One row's sources, in order: a message and a file fold open, an
 * unrecognised event names its type, and a source id the log no longer
 * holds says so - none of the last two open, since there is nothing
 * more to show. */
function Sources({ sources }: { sources: SourceT[] }) {
  if (sources.length === 0) return null;
  return (
    <div className="flex flex-col gap-1">
      {sources.map((source) => {
        switch (source.kind) {
          case "message":
            return <MessageSource key={source.id} source={source} />;
          case "file":
            return <FileSource key={source.id} source={source} />;
          case "event":
            return (
              <p key={source.id} className="text-[var(--ink-2)]">
                From {source.type} {source.id.slice(0, 8)}.
              </p>
            );
          case "missing":
            return (
              <p key={source.id} className="text-[var(--ink-2)]">
                Cites an event the log no longer has.
              </p>
            );
        }
      })}
    </div>
  );
}

/** Wrong, on a row or on a folded option. */
function Acts({ onDispute }: { onDispute: () => void }) {
  return (
    <div className="mt-1 flex items-center gap-2">
      <button
        type="button"
        onClick={onDispute}
        className="min-h-10 rounded-md border-[1.5px] border-[var(--rule)] px-3.5 py-2 font-bold text-[var(--ink)] hover:border-[var(--object)] hover:text-[var(--object)] focus-visible:border-[var(--object)] focus-visible:text-[var(--object)] active:border-[var(--object)] active:text-[var(--object)]"
      >
        Wrong
      </button>
    </div>
  );
}

function OptionRow({ option, onDispute }: { option: OptionRowT; onDispute: (id: string) => void }) {
  return (
    <li className="grid grid-cols-[auto_1fr] items-start gap-2.5 py-2.5">
      <div className="flex flex-col items-start gap-1 text-[var(--ink-2)]">
        <span className="text-[var(--ink)]">{option.id}</span>
      </div>
      <div>
        <p className="font-serif">
          <i>weighed and lost,</i> {option.name}
        </p>
        {option.why && (
          <p className="font-serif text-[var(--ink-2)]">
            <i>because</i> {option.why}
          </p>
        )}
        <ChangedBy base={option} />
        <Acts onDispute={() => onDispute(option.id)} />
      </div>
    </li>
  );
}

/** One row of the queue: a claim's headline, why, what it replaced or
 * reopens, who last changed it and why, the alternatives weighed
 * against it, and the Wrong act on it and on each alternative.
 * `focused` renders the keyboard-navigation highlight: an inset bar on
 * a phone, an underlined id from 40rem. */
export default function Claim({
  claim,
  focused,
  onDispute,
}: {
  claim: RowT;
  focused: boolean;
  onDispute: (id: string) => void;
}) {
  return (
    <li
      data-id={claim.id}
      className={
        "grid grid-cols-1 gap-2 border-b border-[var(--rule)] py-4" +
        (focused ? " -ml-3 border-l-4 border-l-[var(--focus)] pl-2.5 sm:ml-0 sm:border-l-0 sm:pl-0" : "")
      }
    >
      <div className="flex items-center gap-2.5 text-[var(--ink-2)]">
        <span
          className={
            "font-bold text-[var(--ink)]" +
            (focused ? " sm:underline sm:decoration-[var(--focus)] sm:underline-offset-4" : "")
          }
        >
          {claim.id}
        </span>
        <span>{claim.kind}</span>
      </div>
      <p className="font-serif text-[1.0625rem] leading-relaxed">{claim.name}</p>
      {claim.why && (
        <p className="font-serif text-[var(--ink-2)]">
          <i>because</i> {claim.why}
        </p>
      )}
      {claim.was && (
        <p className="text-[var(--ink-2)]">
          Replaced {claim.was.id}, {claim.was.name}.
        </p>
      )}
      {claim.reopens.map((decision) => (
        <p key={decision.id} className="text-[var(--focus)]">
          It reopens {decision.id}, {decision.name}, which stands until you settle this.
        </p>
      ))}
      <ChangedBy base={claim} addedAt={claim.added_at} />
      <Sources sources={claim.sources} />
      {claim.options.length > 0 && (
        <Fold summary={`${plural(claim.options.length, "One alternative", "alternatives")} weighed and lost`}>
          <ul className="ml-4 mt-2 list-none border-l-2 border-[var(--rule)] pl-4">
            {claim.options.map((option) => (
              <OptionRow key={option.id} option={option} onDispute={onDispute} />
            ))}
          </ul>
        </Fold>
      )}
      <Acts onDispute={() => onDispute(claim.id)} />
    </li>
  );
}
