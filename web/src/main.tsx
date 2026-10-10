import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App from "./App";
import { loadSession, SessionError, SessionPage } from "./SessionPage";
import { loadSessions, Sessions, SessionsError } from "./Sessions";
import "./index.css";

const router = createBrowserRouter([
  {
    Component: App,
    HydrateFallback: () => null,
    children: [
      { index: true, loader: loadSessions, Component: Sessions, ErrorBoundary: SessionsError },
      { path: "sessions/:id", loader: loadSession, Component: SessionPage, ErrorBoundary: SessionError },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
