import { Navigate, createFileRoute } from "@tanstack/solid-router";

export const Route = createFileRoute("/jobs/")({
  component: () => <Navigate to="/scrapes" />,
});
