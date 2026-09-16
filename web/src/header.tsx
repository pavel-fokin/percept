import { ArrowLeft, SearchIcon } from "./icons";
import { PATHS, SEARCH_HASH, navigate } from "./routes";
import ThemeToggle from "./theme-toggle";

/** The product header. Its left side answers where this takes you, not
 * where you are: the index carries the wordmark because there is
 * nowhere above it, and every page under it carries a back control
 * naming the index. The page's own heading already says which page it
 * is, so the header never repeats it. */
export default function Header({ back, search }: { back: boolean; search: boolean }) {
  return (
    <header className="sticky top-0 z-20 border-b border-rule bg-page">
      <div className="mx-auto flex min-h-14 max-w-3xl items-center gap-2 px-4 sm:min-h-16 sm:gap-4 sm:px-8">
        {back ? (
          <a
            href={PATHS.index}
            onClick={(event) => {
              if (plainClick(event)) {
                event.preventDefault();
                navigate(PATHS.index);
              }
            }}
            className="-ml-2 inline-flex min-h-11 shrink-0 items-center gap-2 rounded-sm px-2 text-[0.9375rem] text-muted hover:text-ink"
          >
            <ArrowLeft className="size-4" />
            <span>percept</span>
          </a>
        ) : (
          <span className="shrink-0 text-lg font-medium tracking-tight">
            percept<span className="text-accent">.</span>
          </span>
        )}
        <span className="flex-1" />
        {search && <SearchLink />}
        <ThemeToggle />
      </div>
    </header>
  );
}

/** The way into the event log. A magnifier promises a search, so this
 * lands in the log's search field rather than only at its top: the log
 * is where the whole record is searched, and an icon whose promise
 * needs explaining is the wrong icon. The field's own id carries that
 * intent, so the link means the same thing pasted into an address bar
 * as clicked here. */
function SearchLink() {
  return (
    <a
      href={SEARCH_HREF}
      onClick={(event) => {
        if (plainClick(event)) {
          event.preventDefault();
          navigate(SEARCH_HREF);
        }
      }}
      aria-label="Search the event log"
      className="flex size-11 shrink-0 items-center justify-center text-muted hover:text-ink"
    >
      <SearchIcon className="size-[1.15rem]" />
    </a>
  );
}

/** The log, asked for with its search field focused. */
const SEARCH_HREF = `${PATHS.log}${SEARCH_HASH}`;

/** Whether a click on a link is the plain one the router should take
 * over. A modified click is the reader asking the browser for a new
 * tab or window, and the href is a real path, so letting it through
 * costs nothing and preserves what they asked for. */
function plainClick(event: React.MouseEvent): boolean {
  return event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
}
