import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App, { Column } from "./app";
import { BoardError, BoardPage, loadBoard } from "./pages/board-page";
import { boardPattern } from "./paths";
import { loadProject, Project, ProjectError } from "./pages/project";
import { loadStart, Start, StartError } from "./pages/start";
import "./index.css";

const router = createBrowserRouter([
  {
    Component: App,
    HydrateFallback: () => null,
    children: [
      { path: boardPattern, loader: loadBoard, Component: BoardPage, ErrorBoundary: BoardError },
      {
        Component: Column,
        children: [
          { index: true, loader: loadStart, Component: Start, ErrorBoundary: StartError },
          { path: "*", loader: loadProject, Component: Project, ErrorBoundary: ProjectError },
        ],
      },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
