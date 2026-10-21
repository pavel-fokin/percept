import { Link, Outlet, ScrollRestoration } from "react-router";
import { ThemeSwitch } from "./components/theme-switch";

export default function App() {
  return (
    <>
      <div className="mx-auto max-w-3xl px-4 pb-24 sm:px-6">
        <Header />
        <main>
          <Outlet />
        </main>
      </div>
      <ScrollRestoration />
    </>
  );
}

function Header() {
  return (
    <header className="flex h-15 items-center justify-between sm:h-18">
      <Link to="/" className="font-mono text-xl font-medium tracking-tight">
        percept<span className="text-accent">.</span>
      </Link>
      <div className="-mr-3">
        <ThemeSwitch />
      </div>
    </header>
  );
}
