import { useRef, useState, useSyncExternalStore } from "react";
import { Link, NavLink } from "react-router";
import { ThemeSwitch } from "./theme-switch";
import { IconButton } from "./ui/button";

// Tailwind's md breakpoint. The panel mounts once, so its theme switch
// and an open drawer never outlive a resize across it.
const desktop = window.matchMedia("(min-width: 48rem)");

function subscribe(onChange: () => void) {
  desktop.addEventListener("change", onChange);
  return () => desktop.removeEventListener("change", onChange);
}

/** A column on the left from md up, collapsible to a rail; below md a top bar whose button slides the same panel in. */
export function Sidebar() {
  const isDesktop = useSyncExternalStore(subscribe, () => desktop.matches);

  return isDesktop ? <DesktopSidebar /> : <MobileSidebar />;
}

function DesktopSidebar() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem("sidebar") === "collapsed");

  const toggle = () => {
    localStorage.setItem("sidebar", collapsed ? "expanded" : "collapsed");
    setCollapsed(!collapsed);
  };

  return (
    <header
      className={`sticky top-0 h-dvh flex-none overflow-hidden border-r border-border bg-surface transition-[width] motion-reduce:transition-none ${collapsed ? "w-14" : "w-60"}`}
    >
      <Panel collapsed={collapsed} onToggle={toggle} />
    </header>
  );
}

function MobileSidebar() {
  const drawer = useRef<HTMLDialogElement>(null);

  // A click on the backdrop lands on the dialog itself; a click on a link leaves the page.
  const closeOnClick = (e: React.MouseEvent<HTMLDialogElement>) => {
    if (e.target === e.currentTarget || (e.target as Element).closest("a")) e.currentTarget.close();
  };

  return (
    <header className="sticky top-0 z-10 flex h-15 items-center gap-1 border-b border-border bg-surface px-4">
      <div className="-ml-3">
        <IconButton variant="ghost" label="Expand sidebar" onClick={() => drawer.current?.showModal()}>
          <PanelIcon />
        </IconButton>
      </div>
      <Wordmark />
      <dialog
        ref={drawer}
        onClick={closeOnClick}
        className="h-dvh max-h-none w-72 max-w-full -translate-x-full border-r border-border bg-surface text-text transition-[translate,display,overlay] transition-discrete open:translate-x-0 motion-reduce:transition-none starting:open:-translate-x-full"
      >
        <Panel collapsed={false} onToggle={() => drawer.current?.close()} />
      </dialog>
    </header>
  );
}

function Panel({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  return (
    <div className="flex h-full flex-col p-1.5">
      <div className="flex items-center justify-between">
        {!collapsed && <Wordmark className="px-2.5" />}
        <div className="group">
          <IconButton
            variant="ghost"
            label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            aria-expanded={!collapsed}
            onClick={onToggle}
          >
            {collapsed && <Logo className="size-7 group-hover:hidden group-has-focus-visible:hidden" />}
            <PanelIcon className={collapsed ? "hidden group-hover:block group-has-focus-visible:block" : ""} />
          </IconButton>
        </div>
      </div>

      <nav aria-label="Main" className="mt-4 grid gap-0.5">
        <NavLink
          to="/"
          end
          title={collapsed ? "Home" : undefined}
          className={({ isActive }) => `${item} ${isActive ? "bg-border/50 text-text" : ""}`}
        >
          <HomeIcon />
          <span className={collapsed ? "sr-only" : ""}>Home</span>
        </NavLink>
        {/* Projects are created by `percept init` until the web can create one without a folder. */}
        <button
          type="button"
          disabled
          title={collapsed ? "New project" : undefined}
          className={`${item} disabled:cursor-not-allowed disabled:opacity-50`}
        >
          <PlusIcon />
          <span className={collapsed ? "sr-only" : ""}>New project</span>
        </button>
      </nav>

      <div className="mt-auto">
        <ThemeSwitch />
      </div>
    </div>
  );
}

// An icon at the centre of a rail-wide square, so the rail keeps it in place.
const item = "flex h-11 items-center gap-3 rounded-md px-3.5 text-sm whitespace-nowrap text-muted hover:bg-border/50 hover:text-text";

function Wordmark({ className = "" }: { className?: string }) {
  return (
    <Link to="/" className={`font-mono text-xl font-medium tracking-tight whitespace-nowrap ${className}`}>
      percept<span className="text-accent">.</span>
    </Link>
  );
}

// The favicon with a smaller dot, in the theme's colours so it follows the theme switch.
function Logo({ className }: { className: string }) {
  return (
    <svg viewBox="0 0 64 64" aria-hidden className={className}>
      <rect x="1.15" y="1.15" width="61.7" height="61.7" rx="12" strokeWidth="2.3" className="fill-none stroke-border" />
      <circle cx="46.6" cy="46.6" r="6.4" className="fill-accent" />
    </svg>
  );
}

function PanelIcon({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className={`size-4 ${className}`}>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M6 2.75v10.5" />
    </svg>
  );
}

function HomeIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="size-4 flex-none">
      <path d="M2.75 6.5 8 2.25l5.25 4.25v6.25a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1Z" />
      <path d="M6.25 13.75v-4h3.5v4" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden className="size-4 flex-none">
      <path d="M8 3v10M3 8h10" />
    </svg>
  );
}
