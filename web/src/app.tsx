import { QueryClientProvider } from "@tanstack/solid-query";
import { RouterProvider } from "@tanstack/solid-router";
import type { QueryClient } from "@tanstack/solid-query";

import type { AppRouter } from "./router";

export interface AppProps {
  queryClient: QueryClient;
  router: AppRouter;
}

export function App(props: AppProps) {
  return (
    <QueryClientProvider client={props.queryClient}>
      <RouterProvider router={props.router} />
    </QueryClientProvider>
  );
}
