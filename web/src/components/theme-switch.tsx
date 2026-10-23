import { useState } from "react";

type Theme = "light" | "dark";

/** A sidebar row; `className` gives it the row style, and a collapsed rail keeps only the icon. */
export function ThemeSwitch({ collapsed, className }: { collapsed: boolean; className: string }) {
  const [theme, setTheme] = useState<Theme>(
    document.documentElement.dataset.theme === "light" ? "light" : "dark",
  );

  const next: Theme = theme === "dark" ? "light" : "dark";
  const label = next === "light" ? "Light" : "Dark";

  const toggle = () => {
    document.documentElement.dataset.theme = next;
    setTheme(next);
    localStorage.setItem("theme", next);
  };

  // The icon and label show the theme a click switches to.
  return (
    <button type="button" title={collapsed ? `${label} theme` : undefined} onClick={toggle} className={`${className} cursor-pointer`}>
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="size-4 flex-none">
        {next === "light" ? (
          <>
            <circle cx="8" cy="8" r="3" />
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M3 13l1.4-1.4M11.6 4.4 13 3" />
          </>
        ) : (
          <path d="M13.5 10A6 6 0 0 1 6 2.5a6 6 0 1 0 7.5 7.5Z" />
        )}
      </svg>
      <span className={collapsed ? "sr-only" : ""}>{label} theme</span>
    </button>
  );
}
