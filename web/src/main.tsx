import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App, { AppError, Column } from "./app";
import { loadTree } from "./components/sidebar";
import { Board } from "./pages/board";
import { BoardError, loadBoard } from "./pages/board-page";
import { boardPattern } from "./paths";
import { loadProject, Project, ProjectError } from "./pages/project";
import { Home, HomeError, loadHome } from "./pages/home";
import "./index.css";

const router = createBrowserRouter([
  {
    Component: App,
    loader: loadTree,
    // The root route has no params, so only this refetches the tree when `+ Board` navigates to the new board.
    shouldRevalidate: () => true,
    ErrorBoundary: AppError,
    HydrateFallback: () => null,
    children: [
      { path: boardPattern, loader: loadBoard, Component: Board, ErrorBoundary: BoardError },
      {
        Component: Column,
        children: [
          { index: true, loader: loadHome, Component: Home, ErrorBoundary: HomeError },
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
