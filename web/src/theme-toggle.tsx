import { useState } from "react";
import { Moon, Sun } from "./icons";

type Theme = "light" | "dark";

/** The theme the root carries. The head script resolved it before React
 * mounted and wrote it there, so this reads one attribute rather than
 * asking the OS a second time - see `theme.css`. */
function current(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

/** The header's theme switch. It shows the theme you would move to, not
 * the one you are in, so the glyph is the destination. */
export default function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(current);
  const next: Theme = theme === "light" ? "dark" : "light";

  function flip() {
    document.documentElement.dataset.theme = next;
    try {
      localStorage.setItem("percept-theme", next);
    } catch {
      // A private window keeps nothing; the theme still holds for this view.
    }
    setTheme(next);
  }

  return (
    <button
      type="button"
      onClick={flip}
      aria-label={`Switch to the ${next} theme`}
      className="-mr-2 flex size-11 shrink-0 items-center justify-center text-muted hover:text-ink"
    >
      {next === "light" ? <Sun className="size-[1.15rem]" /> : <Moon className="size-[1.15rem]" />}
    </button>
  );
}
