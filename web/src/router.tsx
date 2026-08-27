import { createRouter } from '@tanstack/solid-router'
import type { QueryClient } from '@tanstack/solid-query'
import type { RouterHistory } from '@tanstack/solid-router'

import { routeTree } from './routeTree.gen'

export interface CreateAppRouterOptions {
  queryClient: QueryClient
  history?: RouterHistory
}

export function createAppRouter(options: CreateAppRouterOptions) {
  return createRouter({
    routeTree,
    context: { queryClient: options.queryClient },
    ...(options.history ? { history: options.history } : {}),
    defaultPreload: 'intent',
    scrollRestoration: true,
  })
}

export type AppRouter = ReturnType<typeof createAppRouter>

declare module '@tanstack/solid-router' {
  interface Register {
    router: AppRouter
  }
}
