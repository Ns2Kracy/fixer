import { Navigate, createFileRoute } from "@tanstack/solid-router";

export const Route = createFileRoute("/jobs/$jobId/")({
  component: () => <Navigate to="/scrapes" />,
});
