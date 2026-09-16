import Header from "./header";
import LogPage from "./log-page";
import Projects from "./projects";
import { useRoute } from "./routes";

/** The shell every page shares: the header, and the page the path
 * names. The header's shape follows the route rather than each page
 * drawing its own, so a page can never disagree with the one above
 * it about where the reader is. */
export default function App() {
  const route = useRoute();

  return (
    <div className="flex min-h-screen flex-col">
      <Header back={route === "log"} search={route === "index"} />
      {route === "log" ? <LogPage /> : <Projects />}
    </div>
  );
}
