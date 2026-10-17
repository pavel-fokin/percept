import type { Reference } from "../api";

export function References({ references }: { references: Reference[] }) {
  return (
    <div className="grid gap-3.5">
      {references.map((reference, index) => (
        <figure key={index} className="min-w-0">
          {"message" in reference ? (
            <>
              <figcaption className="text-[13px] font-medium text-muted">Message</figcaption>
              <blockquote className="mt-0.5 leading-[26px] whitespace-pre-wrap wrap-anywhere">
                {reference.message.quote}
              </blockquote>
            </>
          ) : (
            <>
              <figcaption className="text-[13px] font-medium text-muted wrap-anywhere">
                {reference.file.path}:{reference.file.lines[0]}-{reference.file.lines[1]}
              </figcaption>
              <pre className="mt-1 overflow-x-auto rounded-[10px] bg-surface px-4 py-3.5 font-mono text-[13.5px] leading-[22px]">
                {reference.file.excerpt}
              </pre>
            </>
          )}
        </figure>
      ))}
    </div>
  );
}
