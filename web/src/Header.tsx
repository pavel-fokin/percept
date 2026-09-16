import ThemeToggle from "./ThemeToggle";

/** The product header: the wordmark and controls shared by every view. */
export default function Header() {
  return (
    <header className="sticky top-0 z-20 border-b border-rule bg-page">
      <div className="mx-auto flex min-h-14 max-w-3xl items-center gap-2 px-4 sm:min-h-16 sm:gap-4 sm:px-8">
        <span className="shrink-0 text-lg font-medium tracking-tight">
          percept<span className="text-accent">.</span>
        </span>
        <span className="flex-1" />
        <ThemeToggle />
      </div>
    </header>
  );
}
