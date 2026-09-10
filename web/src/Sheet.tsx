import { useEffect, useRef } from "react";
import { plural } from "./format";

/** Escape closes a sheet; every other key is left to the page under
 * it, since a sheet has its own tab order. */
function useEscape(onClose: () => void) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
}

/** The scrim both sheets sit in: a bottom sheet on a phone, a centred
 * dialog from 40rem - the sketch's `.scrim`/`.sheet`. Clicking the
 * scrim itself, not the sheet, closes it. */
function Scrim({ onClose, children }: { onClose: () => void; children: React.ReactNode }) {
  return (
    <div
      className="fixed inset-0 z-10 flex items-end justify-center bg-black/35 sm:items-center"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        className="w-full max-w-2xl rounded-t-xl bg-[var(--sheet)] p-5 pb-[calc(1rem+env(safe-area-inset-bottom))] sm:max-w-md sm:rounded-xl"
        onSubmit={(event) => event.preventDefault()}
      >
        {children}
      </form>
    </div>
  );
}

/** "Why it is wrong": `id` is wrong, its name, a textarea saved into
 * `value` as the user types, and Save/Close - the sketch's why sheet.
 * `onSave` runs only when `value` is non-blank; Save with a blank
 * value refocuses the textarea instead. */
export function WhySheet({
  id,
  name,
  value,
  onChange,
  onCancel,
  onSave,
}: {
  id: string;
  name: string;
  value: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  useEscape(onCancel);
  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  function submit() {
    if (!value.trim()) {
      textareaRef.current?.focus();
      return;
    }
    onSave();
  }

  return (
    <Scrim onClose={onCancel}>
      <h2 id="sheet-title" className="text-base font-bold">
        {id} <span className="text-[var(--object)]">is wrong</span>
      </h2>
      <p className="mt-2 font-serif text-[1.0625rem] leading-relaxed">{name}</p>
      <label htmlFor="why" className="mt-3.5 block text-[var(--ink-2)]">
        Why it is wrong. Saved when you press Save, and read by the model at its next session here.
      </label>
      <textarea
        ref={textareaRef}
        id="why"
        rows={3}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="mt-1.5 block min-h-[5.5rem] w-full resize-y rounded-md border-[1.5px] border-[var(--rule)] bg-[var(--ground)] p-2.5 font-serif text-[1.0625rem] leading-normal text-[var(--ink)]"
      />
      <div className="mt-3.5 flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="min-h-10 rounded-md border-[1.5px] border-transparent px-3.5 py-2 font-normal text-[var(--ink-3)]"
        >
          Close, keep the text
        </button>
        <button
          type="button"
          onClick={submit}
          className="min-h-10 rounded-md border-[1.5px] border-[var(--object)] bg-[var(--object)] px-3.5 py-2 font-bold text-white"
        >
          Save
        </button>
      </div>
    </Scrim>
  );
}

/** "Mark N claims seen?": the finish sheet Finish opens, naming `count`
 * - the map's claimed headline rows shown when it opened - and
 * `map`'s name. With no claimed rows left, every row in the cut is
 * already judged, and the sheet says so instead of naming a count. */
export function FinishSheet({
  map,
  count,
  onCancel,
  onConfirm,
}: {
  map: string;
  count: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  useEscape(onCancel);
  return (
    <Scrim onClose={onCancel}>
      <h2 id="finish-title" className="text-base font-bold">
        {count === 0 ? "Finish this review?" : `Mark ${plural(count, "one claim", "claims")} seen?`}
      </h2>
      <p className="mt-2 text-[var(--ink-2)]">
        {count === 0 ? (
          "Everything here is judged. Finishing closes this sitting."
        ) : (
          <>
            Every claim to {map} you did not mark becomes seen, in one event. Seen is not undone. An
            alternative folded under a decision stays unjudged.
          </>
        )}
      </p>
      <div className="mt-3.5 flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="min-h-10 rounded-md border-[1.5px] border-transparent px-3.5 py-2 font-normal text-[var(--ink-3)]"
        >
          Cancel
        </button>
        <button
          type="button"
          autoFocus
          onClick={onConfirm}
          className="min-h-10 rounded-md border-[1.5px] border-[var(--ink)] bg-[var(--ink)] px-3.5 py-2 font-bold text-[var(--ground)]"
        >
          Mark seen
        </button>
      </div>
    </Scrim>
  );
}
