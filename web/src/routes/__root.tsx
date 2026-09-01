import type { QueryClient } from "@tanstack/solid-query";
import {
  Link,
  Outlet,
  createRootRouteWithContext,
  isRedirect,
  redirect,
  useLocation,
} from "@tanstack/solid-router";
import { Show } from "solid-js";

import { AppShell } from "../components/app-shell";
import { buttonStyles } from "../components/ui/button";
import { authStatusQuery } from "../lib/auth";

export interface RouterContext {
  queryClient: QueryClient;
}

function RootLayout() {
  const isLogin = useLocation({
    select: (location) => location.pathname === "/login",
  });
  return (
    <Show when={!isLogin()} fallback={<Outlet />}>
      <AppShell />
    </Show>
  );
}

function NotFound() {
  return (
    <section class="max-w-3xl pt-[8vh]" aria-labelledby="not-found-title">
      <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
        404 / Route unavailable
      </p>
      <h1
        class="m-0 max-w-[720px] font-serif text-[clamp(3rem,8vw,6rem)] font-medium leading-[0.94] tracking-[-0.04em]"
        id="not-found-title"
      >
        Page not found
      </h1>
      <p class="my-8 max-w-[480px] text-muted">
        The address does not match a workspace view. Nothing was changed.
      </p>
      <Link class={buttonStyles()} to="/">
        Return to workspace
      </Link>
    </section>
  );
}

export const Route = createRootRouteWithContext<RouterContext>()({
  beforeLoad: async ({ context, location }) => {
    if (location.pathname === "/login") return;
    try {
      const status = await context.queryClient.ensureQueryData(
        authStatusQuery(),
      );
      if (!status.authenticated) {
        throw redirect({
          to: "/login",
          search: { redirect: location.href },
          replace: true,
        });
      }
    } catch (error) {
      if (isRedirect(error)) throw error;
      throw redirect({
        to: "/login",
        search: { redirect: location.href },
        replace: true,
      });
    }
  },
  component: RootLayout,
  notFoundComponent: NotFound,
});
