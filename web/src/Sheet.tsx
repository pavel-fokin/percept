import { useEffect, useRef, useState } from "react";

/** The quiet button style: no border, no fill, muted text - shared by
 * a sheet's Close/Cancel. */
const QUIET =
  "min-h-10 rounded-md border-[1.5px] border-transparent px-3.5 py-2 font-normal text-[var(--ink-3)]";

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
 * scrim itself, not the sheet, closes it. `titleId` points the dialog
 * at its own heading; `actions` is the button row every sheet ends
 * with. */
function Scrim({
  onClose,
  titleId,
  actions,
  children,
}: {
  onClose: () => void;
  titleId: string;
  actions: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div
      className="fixed inset-0 z-10 flex items-end justify-center bg-black/35 sm:items-center"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="w-full max-w-2xl rounded-t-xl bg-[var(--sheet)] p-5 pb-[calc(1rem+env(safe-area-inset-bottom))] sm:max-w-md sm:rounded-xl"
      >
        {children}
        <div className="mt-3.5 flex items-center justify-end gap-2">{actions}</div>
      </div>
    </div>
  );
}

/** "Why it is wrong": `id` is wrong, its name, a textarea starting from
 * `initial`, and Save/Close - the sketch's why sheet. The draft lives
 * here, not in the caller, so a keystroke re-renders only this sheet;
 * `onCancel` and `onSave` both hand the caller the text as it stood,
 * so a caller can keep a closed draft or replace it with a saved one.
 * `onSave` runs only when the text is non-blank; Save with a blank
 * value refocuses the textarea instead. */
export function WhySheet({
  id,
  name,
  initial,
  onCancel,
  onSave,
}: {
  id: string;
  name: string;
  initial: string;
  onCancel: (value: string) => void;
  onSave: (value: string) => void;
}) {
  const [value, setValue] = useState(initial);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  useEscape(() => onCancel(value));
  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  function submit() {
    if (!value.trim()) {
      textareaRef.current?.focus();
      return;
    }
    onSave(value);
  }

  return (
    <Scrim
      onClose={() => onCancel(value)}
      titleId="sheet-title"
      actions={
        <>
          <button type="button" onClick={() => onCancel(value)} className={QUIET}>
            Close, keep the text
          </button>
          <button
            type="button"
            onClick={submit}
            className="min-h-10 rounded-md border-[1.5px] border-[var(--object)] bg-[var(--object)] px-3.5 py-2 font-bold text-white"
          >
            Save
          </button>
        </>
      }
    >
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
        onChange={(event) => setValue(event.target.value)}
        className="mt-1.5 block min-h-[5.5rem] w-full resize-y rounded-md border-[1.5px] border-[var(--rule)] bg-[var(--ground)] p-2.5 font-serif text-[1.0625rem] leading-normal text-[var(--ink)]"
      />
    </Scrim>
  );
}

