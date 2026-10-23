import { useState } from "react";
import { useNavigate } from "react-router";
import { postData, type BoardSummary } from "../api";
import { boardUrl } from "../paths";
import { Button } from "./ui/button";

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
    <Button variant="secondary" large className="first:ml-3" disabled={creating} onClick={() => void create()}>
      + Board
    </Button>
  );
}
