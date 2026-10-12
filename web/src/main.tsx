import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App from "./app";
import { loadPathSessions, PathSessions, PathSessionsError } from "./pages/path-sessions";
import { loadSession, SessionError, SessionPage } from "./pages/session";
import { loadStart, Start, StartError } from "./pages/start";
import "./index.css";

const router = createBrowserRouter([
  {
    Component: App,
    HydrateFallback: () => null,
    children: [
      { index: true, loader: loadStart, Component: Start, ErrorBoundary: StartError },
      { path: "p/*", loader: loadPathSessions, Component: PathSessions, ErrorBoundary: PathSessionsError },
      { path: "sessions/:id", loader: loadSession, Component: SessionPage, ErrorBoundary: SessionError },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
