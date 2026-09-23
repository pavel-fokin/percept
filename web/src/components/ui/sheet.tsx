import { useEffect, useRef } from "react";
import type { ReactNode } from "react";

/** A panel that rises from the bottom of a narrow screen, on the native
 * `<dialog>`: the top layer, the inert page behind it, focus kept inside
 * and Escape come with the element, so none of it is written here. A
 * tap on the veil above the panel closes it too. It opens as it mounts;
 * the caller closes it by unmounting it. */
export default function Sheet({
  onClose,
  label,
  children,
}: {
  onClose: () => void;
  label: string;
  children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  return (
    <dialog
      ref={dialog}
      aria-label={label}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      className="m-0 mt-auto max-h-[78vh] w-full max-w-none translate-y-0 overflow-auto rounded-t-2xl border-t border-rule bg-page p-0 text-ink transition-transform duration-200 ease-out backdrop:bg-page/70 starting:open:translate-y-full"
    >
      <div className="px-4 pb-[calc(1.5rem+env(safe-area-inset-bottom,0px))] pt-4">{children}</div>
    </dialog>
  );
}
