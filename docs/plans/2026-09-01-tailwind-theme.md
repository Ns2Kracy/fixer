# Tailwind Components and Three-State Theme Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace Fixer Web's component-level handwritten CSS with Tailwind utilities and genuinely reusable Solid components, and add a persistent system/light/dark theme.

**Architecture:** Semantic CSS variables provide theme-aware Tailwind colors. Pure theme functions handle preference parsing and resolution; a small controller synchronizes DOM, storage, media-query changes, and the theme-color meta tag. Shared UI components own repeated visual contracts while route-specific layouts remain local Tailwind markup.

**Tech Stack:** Solid 2 RC, TanStack Solid Router, Tailwind CSS 4, TypeScript 7, Vitest, Testing Library, Playwright/Chrome DevTools.

---

### Task 1: Add tested theme behavior

**Files:**
- Create: `web/src/lib/theme.ts`
- Create: `web/src/lib/theme.test.ts`
- Modify: `web/src/test/setup.ts`

**Steps:**
1. Write failing unit tests for invalid stored values, system resolution, explicit override, DOM application, persistence, and media-query updates.
2. Run `pnpm --dir web test -- src/lib/theme.test.ts` and confirm failure because the module is missing.
3. Implement the smallest typed theme module with `ThemePreference`, `ResolvedTheme`, `readThemePreference`, `resolveTheme`, `applyTheme`, and `createThemeController`.
4. Add deterministic `matchMedia` test setup only where tests require it.
5. Re-run the targeted test and confirm it passes.
6. Commit as `feat(web): add three-state theme controller`.

### Task 2: Add the reusable theme control and no-flash startup

**Files:**
- Create: `web/src/components/ui/theme-select.tsx`
- Create: `web/src/components/ui/theme-select.test.tsx`
- Modify: `web/src/components/app-shell.tsx`
- Modify: `web/index.html`

**Steps:**
1. Write a failing component test that selects system, light, and dark and observes the preference callback.
2. Run the targeted test and confirm failure.
3. Implement an accessible native select with a visible label for all three choices.
4. Mount a single theme controller in `AppShell`, dispose it on cleanup, and place `ThemeSelect` in the masthead.
5. Add a defensive pre-render initialization script to `web/index.html` using the same storage key and theme values.
6. Run theme tests plus `pnpm --dir web typecheck`.
7. Commit as `feat(web): add persistent theme selector`.

### Task 3: Establish shared Tailwind UI components

**Files:**
- Create: `web/src/components/ui/button.tsx`
- Create: `web/src/components/ui/page-header.tsx`
- Create: `web/src/components/ui/section-header.tsx`
- Create: `web/src/components/ui/form-field.tsx`
- Create: `web/src/components/ui/empty-state.tsx`
- Create: `web/src/components/ui/loading-state.tsx`
- Create: `web/src/components/ui/count-badge.tsx`
- Create: `web/src/components/ui/ui-components.test.tsx`

**Steps:**
1. Write rendering and accessibility tests for variants, labels, headings, counts, empty content, and loading status.
2. Confirm the tests fail because components do not exist.
3. Implement only props demanded by current repeated call sites.
4. Keep route-link styling in `buttonStyles()` instead of a generic polymorphic component.
5. Run targeted tests and typecheck.
6. Commit as `refactor(web): add reusable Tailwind UI components`.

### Task 4: Migrate the shell and domain components

**Files:**
- Modify: `web/src/components/app-shell.tsx`
- Modify: `web/src/components/candidate-picker.tsx`
- Modify: `web/src/components/field-conflict.tsx`
- Modify: `web/src/components/job-status.tsx`
- Modify: `web/src/components/locale-policy-editor.tsx`
- Modify: `web/src/components/output-diff.tsx`
- Modify: `web/src/components/progress-timeline.tsx`
- Modify: `web/src/components/provider-status.tsx`
- Modify: `web/src/components/request-error.tsx`
- Modify: `web/src/components/template-preview.tsx`

**Steps:**
1. Replace semantic presentation classes with Tailwind utilities while preserving DOM semantics and behavior.
2. Replace generated state class names with explicit static state-to-class maps.
3. Reuse shared UI components where at least two call sites exist.
4. Run component/app tests and typecheck.
5. Commit as `refactor(web): migrate shared components to Tailwind`.

### Task 5: Migrate routes to shared components and Tailwind

**Files:**
- Modify: `web/src/routes/__root.tsx`
- Modify: `web/src/routes/index.tsx`
- Modify: `web/src/routes/jobs/index.tsx`
- Modify: `web/src/routes/jobs/$jobId/index.tsx`
- Modify: `web/src/routes/jobs/$jobId/review.tsx`
- Modify: `web/src/routes/jobs/$jobId/plan.tsx`
- Modify: `web/src/routes/library.tsx`
- Modify: `web/src/routes/login.tsx`
- Modify: `web/src/routes/providers.tsx`
- Modify: `web/src/routes/search.tsx`
- Modify: `web/src/routes/settings.tsx`
- Modify: `web/src/routes/templates.tsx`

**Steps:**
1. Migrate workspace routes first, using `PageHeader`, `SectionHeader`, `FormField`, `EmptyState`, `LoadingState`, `CountBadge`, and `Button` where their contracts fit.
2. Run affected route tests and typecheck.
3. Migrate job routes while retaining all state and action behavior.
4. Run affected route tests and typecheck.
5. Commit as `refactor(web): migrate routes to Tailwind components`.

### Task 6: Reduce global CSS to tokens and base rules

**Files:**
- Modify: `web/src/styles.css`

**Steps:**
1. Define semantic light and dark color tokens, font tokens, and global body/focus defaults.
2. Remove all page- and component-level selectors after confirming no TSX references remain.
3. Run a mechanical selector/class usage check.
4. Run the full web test suite, typecheck, and production build.
5. Commit as `refactor(web): reduce global stylesheet to theme tokens`.

### Task 7: Browser verification and review

**Files:**
- Modify only files required by verified defects.

**Steps:**
1. Start the existing Vite development server and use the configured backend or deterministic browser-safe state.
2. Inspect light and dark modes at 320, 768, 1024, and 1440 pixels.
3. Verify theme persistence, system-mode changes, keyboard access, focus visibility, contrast, and console cleanliness.
4. Run code review against the full branch diff and repair all critical or important findings.
5. Re-run `pnpm --dir web test`, `pnpm --dir web typecheck`, and `pnpm --dir web build`.
6. Record final evidence and prepare the branch for integration.
