import { useEffect, useRef, useState } from "react";
import type { Event } from "../api";
import { Button } from "./ui/button";

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

    const below = (id: string) => (document.getElementById(id)?.getBoundingClientRect().top ?? 0) >= window.innerHeight;
    const fresh = messages.find((event) => !known.has(event.id) && below(event.id));
    if (!fresh) return;

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
        <Button
          variant="secondary"
          onClick={jump}
          className="fixed bottom-[calc(1rem+env(safe-area-inset-bottom))] left-1/2 z-10 -translate-x-1/2 whitespace-nowrap"
        >
          <span aria-hidden>↓</span>
          {target.label}
        </Button>
      )}
    </div>
  );
}
