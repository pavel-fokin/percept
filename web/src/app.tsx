import { useState } from "react";
import { Link, Outlet, ScrollRestoration } from "react-router";
import { IconButton } from "./button";

type Theme = "light" | "dark";

export default function App() {
  return (
    <>
      <Header />
      <main className="mx-auto box-content max-w-3xl px-4">
        <Outlet />
      </main>
      <ScrollRestoration />
    </>
  );
}

function Header() {
  return (
    <header className="flex items-center justify-between border-b border-border p-4 md:px-6">
      <h1 className="font-mono text-xl font-medium tracking-tight">
        <Link to="/">
          percept<span className="text-accent">.</span>
        </Link>
      </h1>
      <ThemeSwitch />
    </header>
  );
}

function ThemeSwitch() {
  const [theme, setTheme] = useState<Theme>(
    document.documentElement.dataset.theme === "light" ? "light" : "dark",
  );

  const next: Theme = theme === "dark" ? "light" : "dark";

  const toggle = () => {
    document.documentElement.dataset.theme = next;
    setTheme(next);
    localStorage.setItem("theme", next);
  };

  // The icon shows the theme a click switches to.
  return (
    <IconButton variant="ghost" label={`Switch to ${next} theme`} onClick={toggle}>
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-4">
        {next === "light" ? (
          <>
            <circle cx="8" cy="8" r="3" />
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M3 13l1.4-1.4M11.6 4.4 13 3" />
          </>
        ) : (
          <path d="M13.5 10A6 6 0 0 1 6 2.5a6 6 0 1 0 7.5 7.5Z" />
        )}
      </svg>
    </IconButton>
  );
}
