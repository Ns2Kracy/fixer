import { Link, createRootRouteWithContext } from '@tanstack/solid-router'
import type { QueryClient } from '@tanstack/solid-query'

import { AppShell } from '../components/app-shell'

export interface RouterContext {
  queryClient: QueryClient
}

function NotFound() {
  return (
    <section class="empty-state" aria-labelledby="not-found-title">
      <p class="eyebrow">404 / Route unavailable</p>
      <h1 id="not-found-title">Page not found</h1>
      <p>The address does not match a workspace view. Nothing was changed.</p>
      <Link class="button primary" to="/">Return to workspace</Link>
    </section>
  )
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: AppShell,
  notFoundComponent: NotFound,
})
