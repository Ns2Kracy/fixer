import { createFileRoute } from "@tanstack/solid-router";
import { JobsPage } from "./jobs/index";

export const Route = createFileRoute("/")({ component: OrganizePage });
function OrganizePage() {
  return <JobsPage inbox />;
}
