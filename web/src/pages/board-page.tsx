import type { LoaderFunctionArgs } from "react-router";
import { getData, type BoardSummary } from "../api";
import { gutter } from "../app";
import { RouteError } from "../components/route-error";

/** Loads the board only to tell a missing one apart; its contents are not saved yet. */
export function loadBoard({ params }: LoaderFunctionArgs) {
  const { project, board } = params;
  return getData<BoardSummary>(`/api/projects/${encodeURIComponent(project!)}/boards/${encodeURIComponent(board!)}`);
}

export function BoardError() {
  return (
    <div className={gutter}>
      <RouteError notFound="Board not found." failed="board" />
    </div>
  );
}
