import { Outlet, ScrollRestoration } from "react-router";
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
