import { Chevron } from "./icons";
import ThemeToggle from "./ThemeToggle";

/** The three lenses over one log - only `Everything` is a real screen
 * yet, so the other two render as inactive labels rather than dead
 * links. */
const LENSES = ["Attention", "Conversation", "Everything"] as const;

/** The page's header: the wordmark, the project scope (not yet a
 * menu), and the lens bar with `Everything` current. */
export default function Header() {
  return (
    <header className="sticky top-0 z-20 border-b border-rule bg-page">
      <div className="mx-auto flex min-h-14 max-w-3xl items-center gap-2 px-4 sm:min-h-16 sm:gap-4 sm:px-8">
        <span className="shrink-0 text-lg font-medium tracking-tight">
          percept<span className="text-accent">.</span>
        </span>
        <span className="h-4 w-px shrink-0 bg-rule" />
        <span className="flex min-h-11 min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">All projects</span>
          <Chevron className="size-3 shrink-0 text-faint" />
        </span>
        <span className="flex-1" />
        <ThemeToggle />
      </div>
      <div className="mx-auto -mb-px max-w-3xl overflow-x-auto px-4 sm:px-8">
        <nav aria-label="Lenses" className="flex min-w-max items-center gap-6 text-[0.8125rem]">
          {LENSES.map((lens) => (
            <span
              key={lens}
              aria-current={lens === "Everything" ? "page" : undefined}
              className={
                "flex min-h-11 items-center border-b-2 " +
                (lens === "Everything" ? "border-accent text-ink" : "border-transparent text-muted")
              }
            >
              {lens}
            </span>
          ))}
        </nav>
      </div>
    </header>
  );
}
