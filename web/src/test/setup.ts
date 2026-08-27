import '@testing-library/jest-dom/vitest'

import { afterEach, vi } from 'vitest'

import { cleanup } from './render'

Object.defineProperty(window, 'scrollTo', { value: vi.fn(), writable: true })

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})
