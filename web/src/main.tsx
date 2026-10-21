import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App, { Column } from "./app";
import { Board } from "./pages/board";
import { loadProject, Project, ProjectError } from "./pages/project";
import { loadStart, Start, StartError } from "./pages/start";
import "./index.css";

const router = createBrowserRouter([
  {
    Component: App,
    HydrateFallback: () => null,
    children: [
      { path: "board", Component: Board },
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
