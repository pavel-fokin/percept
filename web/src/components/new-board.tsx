import { useState } from "react";
import { useNavigate } from "react-router";
import type { BoardSummary } from "../api";
import { boardUrl } from "../paths";

/** A `+ Board` button that turns into a name input; Enter creates the board and opens it. */
export function NewBoard({ project }: { project: string }) {
  const navigate = useNavigate();
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const cancel = () => {
    setNaming(false);
    setName("");
    setError(null);
  };

  const create = async () => {
    const res = await fetch(`/api/projects/${encodeURIComponent(project)}/boards`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
    });
    if (!res.ok) {
      setError((await res.text()) || `HTTP ${res.status}`);
      return;
    }

    const board = ((await res.json()) as { data: BoardSummary }).data;
    navigate(boardUrl(project, board.name));
  };

  if (!naming) {
    return (
      <button
        type="button"
        onClick={() => setNaming(true)}
        className="min-h-11 cursor-pointer rounded-lg px-3 py-2.5 text-base text-muted hover:bg-surface hover:text-text focus-visible:bg-surface"
      >
        + Board
      </button>
    );
  }

  return (
    <div className="grid min-w-0 flex-1 gap-1 px-3 sm:flex-none">
      <input
        autoFocus
        aria-label="Board name"
        placeholder="Board name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => name === "" && cancel()}
        onKeyDown={(e) => {
          if (e.key === "Escape") cancel();
          if (e.key === "Enter") void create();
        }}
        className="h-11 w-full rounded-lg border border-border bg-bg px-3 text-base sm:w-64"
      />
      {error && <p role="alert" className="text-sm wrap-anywhere">{error}</p>}
    </div>
  );
}
