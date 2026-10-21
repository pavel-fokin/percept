import { useState } from "react";
import { useNavigate } from "react-router";
import { postData, type BoardSummary } from "../api";
import { boardUrl } from "../paths";
import { chip } from "./ui/chip";

/** A `+ Board` button that creates a board in `project` at once and opens it. */
export function NewBoard({ project }: { project: string }) {
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

  const create = async () => {
    if (creating) return;
    setCreating(true);

    try {
      const board = await postData<BoardSummary>(`/api/projects/${encodeURIComponent(project)}/boards`);
      navigate(boardUrl(project, board.name));
    } catch {
      setCreating(false);
    }
  };

  return (
    <button
      type="button"
      disabled={creating}
      onClick={() => void create()}
      className={`${chip} cursor-pointer text-base text-muted hover:text-text`}
    >
      + Board
    </button>
  );
}
