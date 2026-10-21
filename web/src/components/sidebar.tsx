import { useState } from "react";
import { Link } from "react-router";
import { ThemeSwitch } from "./theme-switch";
import { IconButton } from "./ui/button";

/** A column on the left from md up, collapsible to a rail; below md a top bar that never collapses. */
export function Sidebar() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem("sidebar") === "collapsed");

  const toggle = () => {
    localStorage.setItem("sidebar", collapsed ? "expanded" : "collapsed");
    setCollapsed(!collapsed);
  };

  return (
    <header
      className={`sticky top-0 z-10 flex h-15 flex-none items-center justify-between border-b border-border bg-surface px-4 transition-[width] motion-reduce:transition-none md:h-dvh md:flex-col md:items-start md:overflow-hidden md:border-r md:border-b-0 md:p-1.5 ${collapsed ? "md:w-14" : "md:w-60"}`}
    >
      <div className="flex items-center justify-between md:w-full">
        <Link
          to="/"
          className={`font-mono text-xl font-medium tracking-tight whitespace-nowrap md:px-2.5 ${collapsed ? "md:hidden" : ""}`}
        >
          percept<span className="text-accent">.</span>
        </Link>
        <div className="group hidden md:block">
          <IconButton
            variant="ghost"
            label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            aria-expanded={!collapsed}
            onClick={toggle}
          >
            {collapsed ? (
              <>
                <Logo className="size-7 group-hover:hidden group-has-focus-visible:hidden" />
                <PanelIcon className="hidden group-hover:block group-has-focus-visible:block" />
              </>
            ) : (
              <PanelIcon />
            )}
          </IconButton>
        </div>
      </div>
      <div className="-mr-3 md:mr-0">
        <ThemeSwitch />
      </div>
    </header>
  );
}

// The favicon with a smaller dot, in the theme's colours so it follows the theme switch.
function Logo({ className }: { className: string }) {
  return (
    <svg viewBox="0 0 64 64" aria-hidden className={className}>
      <rect x="1.15" y="1.15" width="61.7" height="61.7" rx="12" strokeWidth="2.3" className="fill-bg stroke-border" />
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
