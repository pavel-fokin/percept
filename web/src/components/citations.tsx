import type { Citation } from "../api";

/** The snapshot each citation took from its source. A file citation never opens the file as it is now. */
export function Citations({ citations }: { citations: Citation[] }) {
  if (citations.length === 0) return null;

  return (
    <ul className="grid min-w-0 gap-4">
      {citations.map((citation, i) => (
        <li key={i} className="min-w-0">
          {"message" in citation ? (
            <blockquote className="border-l-2 border-border pl-3 leading-[26px] wrap-anywhere whitespace-pre-line">{citation.message.quote}</blockquote>
          ) : (
            <figure>
              <figcaption className="font-mono text-[13px] wrap-anywhere text-muted">
                {citation.file.path}:{citation.file.lines[0]}-{citation.file.lines[1]}
              </figcaption>
              <pre className="mt-1 overflow-x-auto rounded-[10px] bg-surface px-3 py-2 font-mono text-[13px] leading-5">{citation.file.excerpt}</pre>
            </figure>
          )}
        </li>
      ))}
    </ul>
  );
}
