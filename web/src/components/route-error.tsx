import { isRouteErrorResponse, Link, useRouteError } from "react-router";
import { describe } from "../api";
import { textLink } from "./ui/text-link";

/** A route's error: what was not found, with a way back, or why loading failed. */
export function RouteError({ notFound, failed }: { notFound: string; failed: string }) {
  const error = useRouteError();

  if (isRouteErrorResponse(error) && (error.status === 404 || error.status === 400)) {
    return (
      <p className="pt-6">
        {notFound}{" "}
        <Link to="/" className={textLink}>
          All projects
        </Link>
      </p>
    );
  }

  return <p className="pt-6">Failed to load {failed}: {describe(error)}</p>;
}
