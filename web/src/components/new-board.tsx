import { useState } from "react";
import { useNavigate } from "react-router";
import { describe, postData, type BoardSummary } from "../api";
import { boardUrl } from "../paths";
import { chip } from "./ui/chip";

/** A `+ Board` button that turns into a name input; Enter creates the board and opens it. */
export function NewBoard({ project }: { project: string }) {
  const navigate = useNavigate();
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const cancel = () => {
    setNaming(false);
    setName("");
    setError(null);
  };

  const create = async () => {
    if (creating) return;
    setCreating(true);

    try {
      const board = await postData<BoardSummary>(`/api/projects/${encodeURIComponent(project)}/boards`, { name });
      navigate(boardUrl(project, board.name));
    } catch (e) {
      setError(describe(e));
      setCreating(false);
    }
  };

  if (!naming) {
    return (
      <button
        type="button"
        onClick={() => setNaming(true)}
        className={`${chip} cursor-pointer text-base text-muted hover:text-text`}
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
        onChange={(e) => {
          setName(e.target.value);
          setError(null);
        }}
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
