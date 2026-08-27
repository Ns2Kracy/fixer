import { render, screen, waitFor } from './test/render'
import userEvent from '@testing-library/user-event'
import { QueryClient } from '@tanstack/solid-query'
import { createMemoryHistory } from '@tanstack/solid-router'
import { describe, expect, it, vi } from 'vitest'

import { App } from './app'
import { createAppRouter } from './router'

function renderApp(initialEntry = '/') {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [initialEntry] }),
    queryClient,
  })

  return render(() => <App queryClient={queryClient} router={router} />)
}

describe('Fixer workspace', () => {
  it('mounts the workspace dashboard', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ schema_version: 1, status: 'ok', version: '0.1.0' }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    )

    renderApp()

    expect(await screen.findByRole('heading', { name: 'Metadata work, without guesswork.' })).toBeVisible()
    expect(await screen.findByText('Server connected')).toBeVisible()
    expect(screen.getByRole('link', { name: 'Review workspace' })).toHaveAttribute('href', '#activity-title')
  })

  it('recovers from an unknown route through client-side navigation', async () => {
    vi.stubGlobal('fetch', vi.fn())
    const user = userEvent.setup()
    renderApp('/missing')

    expect(await screen.findByRole('heading', { name: 'Page not found' })).toBeVisible()
    const workspaceLink = screen.getByRole('link', { name: 'Return to workspace' })
    workspaceLink.focus()
    await user.keyboard('{Enter}')

    expect(await screen.findByRole('heading', { name: 'Metadata work, without guesswork.' })).toBeVisible()
  })

  it('shows the structured API error instead of a generic failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: {
              code: 'authentication_required',
              message: 'Authentication is required',
              request_id: 'req-0000000000000001',
            },
          }),
          { status: 401, headers: { 'content-type': 'application/json' } },
        ),
      ),
    )

    renderApp()

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Authentication is required')
    expect(alert).toHaveTextContent('req-0000000000000001')
  })

  it('offers a keyboard skip link that moves focus to main content', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ schema_version: 1, status: 'ok', version: '0.1.0' }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    )
    const user = userEvent.setup()
    renderApp()

    await screen.findByRole('heading', { name: 'Metadata work, without guesswork.' })
    await user.tab()
    expect(screen.getByRole('link', { name: 'Skip to workspace' })).toHaveFocus()
    await user.keyboard('{Enter}')

    await waitFor(() => expect(screen.getByRole('main')).toHaveFocus())
  })
})
