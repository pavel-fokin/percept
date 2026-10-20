import { useEffect } from "react";
import { useRevalidator } from "react-router";

const INTERVAL_MS = 5000;

/** While `active`, revalidates the route loaders every 5 s while the tab is visible, never overlapping requests. */
export function usePolling(active: boolean) {
  const { revalidate, state } = useRevalidator();

  useEffect(() => {
    if (!active || state !== "idle") return;

    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      if (document.visibilityState === "hidden") return;
      timer = setTimeout(() => void revalidate(), INTERVAL_MS);
    };
    const onVisibility = () => {
      clearTimeout(timer);
      if (document.visibilityState === "visible") void revalidate();
    };

    schedule();
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [active, revalidate, state]);
}
