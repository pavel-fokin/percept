import ThemeToggle from "./ThemeToggle";

/** The page's header: the wordmark, the project the log is scoped to,
 * and the theme switch. The project is the page's subject and its
 * heading - the server serves one project, so the scope is stated here
 * once rather than on every row. */
export default function Header({ project }: { project: string | null }) {
  return (
    <header className="sticky top-0 z-20 border-b border-rule bg-page">
      <div className="mx-auto flex min-h-14 max-w-3xl items-center gap-2 px-4 sm:min-h-16 sm:gap-4 sm:px-8">
        <span className="shrink-0 text-lg font-medium tracking-tight">
          percept<span className="text-accent">.</span>
        </span>
        {project && (
          <>
            <span className="h-4 w-px shrink-0 bg-rule" />
            <h1 className="min-w-0 truncate text-sm font-medium">{project}</h1>
          </>
        )}
        <span className="flex-1" />
        <ThemeToggle />
      </div>
    </header>
  );
}
