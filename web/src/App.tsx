import { useEffect, useState } from "react";

type Status = { events: number };
type Theme = "light" | "dark";

export default function App() {
  const [status, setStatus] = useState<Status>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    fetch("/api/status")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<{ data: Status }>;
      })
      .then((body) => setStatus(body.data))
      .catch((e: Error) => setError(e.message));
  }, []);

  return (
    <div className="mx-auto max-w-5xl px-4">
      <Header />
      <main className="py-8">
        {error ? (
          <p>Failed to load status: {error}</p>
        ) : status ? (
          <p>{status.events} events</p>
        ) : (
          <p className="text-muted">Loading...</p>
        )}
      </main>
    </div>
  );
}

function Header() {
  return (
    <header className="flex items-center justify-between border-b border-border py-4">
      <span className="text-xl font-semibold tracking-tight">
        percept<span className="text-accent">.</span>
      </span>
      <ThemeSwitch />
    </header>
  );
}

function ThemeSwitch() {
  const [theme, setTheme] = useState<Theme>(
    document.documentElement.dataset.theme === "light" ? "light" : "dark",
  );

  const choose = (next: Theme) => {
    document.documentElement.dataset.theme = next;
    localStorage.setItem("theme", next);
    setTheme(next);
  };

  return (
    <div role="group" aria-label="Theme" className="inline-flex gap-0.5 rounded-md border border-border p-0.5">
      <ThemeButton label="Light theme" pressed={theme === "light"} onClick={() => choose("light")}>
        <circle cx="8" cy="8" r="3" />
        <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M3 13l1.4-1.4M11.6 4.4 13 3" />
      </ThemeButton>
      <ThemeButton label="Dark theme" pressed={theme === "dark"} onClick={() => choose("dark")}>
        <path d="M13.5 10A6 6 0 0 1 6 2.5a6 6 0 1 0 7.5 7.5Z" />
      </ThemeButton>
    </div>
  );
}

function ThemeButton(props: {
  label: string;
  pressed: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={props.label}
      aria-pressed={props.pressed}
      onClick={props.onClick}
      className="cursor-pointer rounded px-2.5 py-1.5 text-muted aria-pressed:bg-surface aria-pressed:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    >
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-3.5">
        {props.children}
      </svg>
    </button>
  );
}
