import { Navigate, createFileRoute } from "@tanstack/solid-router";

export const Route = createFileRoute("/jobs/$jobId/plan")({
  component: () => <Navigate to="/scrapes" />,
});
