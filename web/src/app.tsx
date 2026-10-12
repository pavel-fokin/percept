import { Link, Outlet, ScrollRestoration } from "react-router";
import { ThemeSwitch } from "./components/theme-switch";

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
