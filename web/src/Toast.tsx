/** The bottom-centred confirmation a save shows for four seconds - the
 * sketch's `#toast`. `App` owns the timer; this only renders what it is
 * given. */
export default function Toast({ text }: { text: string | null }) {
  if (!text) return null;
  return (
    <div
      role="status"
      aria-live="polite"
      className="fixed bottom-[4.75rem] left-1/2 z-[7] -translate-x-1/2 rounded-md bg-[var(--ink)] px-3.5 py-2 text-[var(--ground)]"
    >
      {text}
    </div>
  );
}
