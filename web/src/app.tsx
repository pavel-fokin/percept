import { Link, Outlet, ScrollRestoration } from "react-router";
import { ThemeSwitch } from "./components/theme-switch";

export default function App() {
  return (
    <>
      <div className="flex min-h-dvh flex-col">
        <Header />
        <main className="flex flex-1 flex-col">
          <Outlet />
        </main>
      </div>
      <ScrollRestoration />
    </>
  );
}

/** The reading width for pages of text; a page that needs the full width sits outside it. */
export function Column() {
  return (
    <div className="mx-auto w-full max-w-3xl px-4 pb-24 sm:px-6">
      <Outlet />
    </div>
  );
}

function Header() {
  return (
    <header className="mx-auto flex h-15 w-full max-w-3xl items-center justify-between px-4 sm:h-18 sm:px-6">
      <Link to="/" className="font-mono text-xl font-medium tracking-tight">
        percept<span className="text-accent">.</span>
      </Link>
      <div className="-mr-3">
        <ThemeSwitch />
      </div>
    </header>
  );
}
