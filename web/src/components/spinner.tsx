export function Spinner() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden className="size-3 flex-none motion-safe:animate-spin">
      <circle cx="8" cy="8" r="6" opacity="0.3" />
      <path d="M14 8a6 6 0 0 0-6-6" />
    </svg>
  );
}
