import { useEffect, useState } from "react";

type Theme = "light" | "dark";

const README = "https://github.com/pavel-fokin/percept#readme";

export default function App() {
  return (
    <div className="mx-auto max-w-5xl px-4">
      <Header />
      <main className="py-8">
        <Sessions />
      </main>
    </div>
  );
}

function Sessions() {
  const [sessions, setSessions] = useState<unknown[]>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    fetch("/api/sessions")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<{ data: unknown[] }>;
      })
      .then((body) => setSessions(body.data))
      .catch((e: Error) => setError(e.message));
  }, []);

  if (error) return <p>Failed to load sessions: {error}</p>;
  if (!sessions) return null;
  if (sessions.length === 0) return <EmptyState />;

  return (
    <p className="text-muted">
      {sessions.length} {sessions.length === 1 ? "session" : "sessions"}
    </p>
  );
}

function EmptyState() {
  return (
    <section className="grid justify-items-center gap-3 py-16 text-center">
      <h2 className="text-2xl font-semibold text-balance">No sessions yet</h2>
      <p className="max-w-[36ch] text-muted">
        Sessions appear here once a coding client sends its first hook event.
      </p>
      <a href={README} className="mt-2 underline decoration-muted underline-offset-3 hover:decoration-text">
        Set up a client
      </a>
    </section>
  );
}

function Header() {
  return (
    <header className="flex items-center justify-between border-b border-border py-4">
      <h1 className="font-mono text-xl font-medium tracking-tight">
        percept<span className="text-accent">.</span>
      </h1>
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
    setTheme(next);
    localStorage.setItem("theme", next);
  };

  return (
    <div role="group" aria-label="Theme" className="inline-flex gap-0.5 rounded-md border border-border p-0.5">
      <ThemeButton theme="light" current={theme} onChoose={choose}>
        <circle cx="8" cy="8" r="3" />
        <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M3 13l1.4-1.4M11.6 4.4 13 3" />
      </ThemeButton>
      <ThemeButton theme="dark" current={theme} onChoose={choose}>
        <path d="M13.5 10A6 6 0 0 1 6 2.5a6 6 0 1 0 7.5 7.5Z" />
      </ThemeButton>
    </div>
  );
}

function ThemeButton(props: {
  theme: Theme;
  current: Theme;
  onChoose: (theme: Theme) => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={`${props.theme} theme`}
      aria-pressed={props.theme === props.current}
      onClick={() => props.onChoose(props.theme)}
      className="cursor-pointer rounded px-2.5 py-1.5 text-muted aria-pressed:bg-surface aria-pressed:text-text"
    >
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="size-3.5">
        {props.children}
      </svg>
    </button>
  );
}
