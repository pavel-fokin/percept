import { useEffect } from "react";
import { useRevalidator } from "react-router";

/** Revalidates the route loaders every `ms` while the tab is visible, never overlapping requests. */
export function usePolling(ms: number) {
  const { revalidate, state } = useRevalidator();

  useEffect(() => {
    if (state !== "idle") return;

    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      if (document.visibilityState === "hidden") return;
      timer = setTimeout(() => void revalidate(), ms);
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
  }, [ms, revalidate, state]);
}
