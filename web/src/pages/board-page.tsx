import type { LoaderFunctionArgs } from "react-router";
import { getData, type BoardDetail, type MapSummary } from "../api";
import { gutter } from "../app";
import { RouteError } from "../components/route-error";

/** The board with the maps on it, and the maps of its project that could be added. */
export async function loadBoard({ params }: LoaderFunctionArgs) {
  const { project, board } = params;
  const [detail, maps] = await Promise.all([
    getData<BoardDetail>(`/api/projects/${encodeURIComponent(project!)}/boards/${encodeURIComponent(board!)}`),
    getData<MapSummary[]>("/api/maps"),
  ]);
  const shown = new Set(detail.maps.map((m) => m.name));

  return { board: detail, addable: maps.filter((m) => m.project === detail.project && !shown.has(m.name)) };
}

export function BoardError() {
  return (
    <div className={gutter}>
      <RouteError notFound="Board not found." failed="board" />
    </div>
  );
}
