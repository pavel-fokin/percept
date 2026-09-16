import { useEffect, useRef } from "react";
import { Chevron } from "./icons";

export interface FilterMenuOption<T extends string | null> {
  value: T;
  label: string;
}

/** One chip-and-panel filter menu - the kind menu and the time menu are
 * both this, since the only difference between them is their options
 * and whether picking one closes the panel. A `<details>` for its
 * semantics and keyboard focus, but the summary's click is the only
 * thing that opens or closes it: `open` is the caller's state, so
 * letting the platform's own toggle report back would make a
 * programmatic close look like a user's and leave the two menus
 * undoing each other. */
export default function FilterMenu<T extends string | null>({
  chipLabel,
  options,
  isSelected,
  onSelect,
  multi,
  open,
  onOpenChange,
}: {
  chipLabel: string;
  options: FilterMenuOption<T>[];
  isSelected: (value: T) => boolean;
  onSelect: (value: T) => void;
  multi: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const ref = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: PointerEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) onOpenChange(false);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onOpenChange(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onOpenChange]);

  function handleSelect(value: T) {
    onSelect(value);
    if (!multi) onOpenChange(false);
  }

  return (
    <details ref={ref} open={open} className="relative">
      <summary
        onClick={(event) => {
          event.preventDefault();
          onOpenChange(!open);
        }}
        className="inline-flex min-h-11 items-center gap-1.5 rounded-sm border border-rule px-3 text-[0.8125rem] text-muted hover:text-ink"
      >
        <span>{chipLabel}</span>
        <span data-chev className="flex items-center">
          <Chevron className="size-3 text-faint" />
        </span>
      </summary>
      {/* `min-w-max` so the panel is as wide as its longest label: sized
          to the chip instead, every two-word kind wrapped. */}
      <div className="absolute top-full z-30 mt-1 max-h-[70vh] min-w-max overflow-y-auto rounded-sm border border-rule bg-panel py-1.5 shadow-lg">
        {options.map((option) => (
          <button
            key={String(option.value)}
            type="button"
            aria-selected={isSelected(option.value)}
            onClick={() => handleSelect(option.value)}
            className={`flex min-h-11 w-full items-center px-4 text-left text-sm whitespace-nowrap ${
              isSelected(option.value) ? "text-accent" : "text-ink"
            }`}
          >
            {option.label}
          </button>
        ))}
      </div>
    </details>
  );
}
