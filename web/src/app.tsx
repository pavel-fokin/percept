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

const column = "mx-auto w-full max-w-3xl px-4 sm:px-6";

/** The reading width for pages of text; a page that needs the full width sits outside it. */
export function Column() {
  return (
    <div className={`${column} pb-24`}>
      <Outlet />
    </div>
  );
}

function Header() {
  return (
    <header className={`${column} flex h-15 items-center justify-between sm:h-18`}>
      <Link to="/" className="font-mono text-xl font-medium tracking-tight">
        percept<span className="text-accent">.</span>
      </Link>
      <div className="-mr-3">
        <ThemeSwitch />
      </div>
    </header>
  );
}
