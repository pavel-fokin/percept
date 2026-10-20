import { useEffect, useRef, useState } from "react";
import type { Event } from "../api";

type Target = { id: string; label: string };

/** Offers a jump to the earliest message that landed below the viewport since the page was opened. Tool and subagent lines never count. */
export function NewMessage({ events }: { events: Event[] }) {
  const seen = useRef<Set<string> | null>(null);
  const [target, setTarget] = useState<Target | null>(null);

  useEffect(() => {
    const messages = events.filter((event) => event.kind === "message");
    const known = seen.current;
    seen.current = new Set(messages.map((event) => event.id));
    if (!known) return;

    const fresh = messages.find((event) => !known.has(event.id));
    if (!fresh) return;

    const element = document.getElementById(fresh.id);
    if (!element || element.getBoundingClientRect().top < window.innerHeight) return;

    const label = fresh.actor === "human" ? "New prompt" : "New reply";
    setTarget((current) => current ?? { id: fresh.id, label });
  }, [events]);

  useEffect(() => {
    const element = target && document.getElementById(target.id);
    if (!element) return;

    const observer = new IntersectionObserver(([entry]) => entry.isIntersecting && setTarget(null));
    observer.observe(element);

    return () => observer.disconnect();
  }, [target]);

  function jump() {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    document.getElementById(target!.id)?.scrollIntoView({ block: "start", behavior: reduced ? "auto" : "smooth" });
    setTarget(null);
  }

  return (
    <div role="status">
      {target && (
        <button
          type="button"
          onClick={jump}
          className="fixed bottom-[calc(1rem+env(safe-area-inset-bottom))] left-1/2 z-10 min-h-11 -translate-x-1/2 cursor-pointer rounded-full border border-border bg-surface px-4 text-sm whitespace-nowrap text-text"
        >
          <span aria-hidden>↓ </span>
          {target.label}
        </button>
      )}
    </div>
  );
}
