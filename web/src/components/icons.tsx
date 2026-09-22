/** The glyphs the log screen draws. Kept together since none carries
 * state - each is the mock's inline SVG, ported as-is. */

export function Chevron({ className }: { className: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

export function SearchIcon({ className }: { className: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" className={className} aria-hidden="true">
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.6-3.6" />
    </svg>
  );
}

export function ArrowLeft({ className }: { className: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="M19 12H5m0 0 6-6m-6 6 6 6" />
    </svg>
  );
}

export function Sun({ className }: { className: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" className={className} aria-hidden="true">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </svg>
  );
}

/** One monochrome shape per node kind, cycling through five, chosen by
 * the kind's index in the schema - never by colour, so the tree stays
 * inside the page's one accent hue. */
const KIND_SHAPES = [
  <circle key="0" cx="6" cy="6" r="4" fill="currentColor" />,
  <rect key="1" x="2.5" y="2.5" width="7" height="7" fill="currentColor" transform="rotate(45 6 6)" />,
  <circle key="2" cx="6" cy="6" r="3.5" fill="none" stroke="currentColor" strokeWidth="1.4" />,
  <rect key="3" x="2.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1.4" />,
  <path key="4" d="M6 2 10 9.5H2Z" fill="currentColor" />,
];

export function KindGlyph({ index, className }: { index: number; className?: string }) {
  return (
    <svg viewBox="0 0 12 12" className={className} aria-hidden="true">
      {KIND_SHAPES[index % KIND_SHAPES.length]}
    </svg>
  );
}

export function CloseIcon({ className }: { className: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" className={className} aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  );
}

export function Moon({ className }: { className: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
    </svg>
  );
}
