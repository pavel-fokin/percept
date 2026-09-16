import { Outlet, Route, Routes } from "react-router";
import Header from "./header";
import LogPage from "./log-page";
import MapView from "./map";
import Projects from "./projects";
import { PATHS } from "./routes";

/** The shell every page shares: the header, and the page the path
 * names. The header is drawn once, here, rather than by each page, so
 * a page can never disagree with the one above it about where the
 * reader is.
 *
 * An unrecognised path falls to the index. The server hands this page
 * every address it does not own itself, so a typed or stale URL has to
 * land somewhere real rather than on a blank screen. */
export default function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route path={PATHS.index} element={<Projects />} />
        <Route path={PATHS.log} element={<LogPage />} />
        <Route path={PATHS.map} element={<MapView />} />
        <Route path="*" element={<Projects />} />
      </Route>
    </Routes>
  );
}

function Shell() {
  return (
    <div className="flex min-h-screen flex-col">
      <Header />
      <Outlet />
    </div>
  );
}
