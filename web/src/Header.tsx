import { Chevron } from "./icons";
import ThemeToggle from "./ThemeToggle";

/** The page's header: the wordmark, the project scope (not yet a
 * menu), and the theme switch. There is one screen, so there is no
 * lens bar to choose between. */
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
    </header>
  );
}
