import { useLoaderData, type LoaderFunctionArgs } from "react-router";
import { getData, type BoardSummary } from "../api";
import { RouteError } from "../components/route-error";
import { Board } from "./board";

export function loadBoard({ params }: LoaderFunctionArgs) {
  const { project, board } = params;
  return getData<BoardSummary>(`/api/projects/${encodeURIComponent(project!)}/boards/${encodeURIComponent(board!)}`);
}

export function BoardPage() {
  // The board's contents are not saved yet; its identity is all the page takes from the server.
  useLoaderData<typeof loadBoard>();

  return <Board />;
}

export function BoardError() {
  return <RouteError notFound="Board not found." failed="board" />;
}
