/** The arrow of a disclosure row. `open` names the details group it turns with, so a nested chevron ignores the outer one. */
export function Chevron({ open = "group-open:rotate-90", className = "" }: { open?: string; className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      aria-hidden
      className={`size-3.5 flex-none transition-transform ${open} ${className}`}
    >
      <path d="m6 3 5 5-5 5" />
    </svg>
  );
}
