import { Outlet, ScrollRestoration, useRouteError } from "react-router";
import { describe } from "./api";
import { Sidebar } from "./components/sidebar";

export default function App() {
  return (
    <>
      <div className="flex min-h-dvh flex-col md:flex-row">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <Outlet />
        </main>
      </div>
      <ScrollRestoration />
    </>
  );
}

/** The sidebar's data failed to load, so no page can render beside it. */
export function AppError() {
  return <p className={`pt-6 ${gutter}`}>Failed to load projects: {describe(useRouteError())}</p>;
}

/** The side gutter of every page. */
export const gutter = "px-4 sm:px-6";

/** The reading width for pages of text; a page that needs the full width sits outside it. */
export function Column() {
  return (
    <div className={`mx-auto w-full max-w-3xl pb-24 ${gutter}`}>
      <Outlet />
    </div>
  );
}
