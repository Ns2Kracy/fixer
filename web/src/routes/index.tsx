import { createFileRoute } from "@tanstack/solid-router";

import { ScrapeRunsPage } from "../components/scrape-runs-page";

export const Route = createFileRoute("/")({ component: OrganizePage });
function OrganizePage() {
  return <ScrapeRunsPage create />;
}
