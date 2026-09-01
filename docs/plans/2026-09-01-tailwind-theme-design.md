# Tailwind component migration and theme design

**Date:** 2026-09-01

## Goal

Move Fixer Web from a large semantic stylesheet to Tailwind utilities and reusable Solid components, while adding a persistent light/dark/system theme without changing routes, data flow, or user-facing behavior.

## Styling boundary

`web/src/styles.css` retains only:

- `@import "tailwindcss"`
- theme color and font tokens
- light and dark semantic token values
- global element defaults that truly apply application-wide

Page layout, component appearance, responsive behavior, interaction states, and pseudo-elements live in TSX Tailwind classes. Generated state class names are replaced with explicit static class maps so Tailwind can discover every class.

## Component boundary

Only repeated UI patterns become shared components:

- `Button` plus a class helper for router links
- `PageHeader`
- `SectionHeader`
- `FormField`
- `EmptyState`
- `LoadingState`
- `CountBadge`
- `ThemeSelect`

Route-specific grids, panels, and domain presentation remain local until they have at least two real consumers.

## Theme behavior

Theme preference is `system`, `light`, or `dark`. The default is `system`. A manual choice is stored under `fixer-theme`. The resolved theme is written to `document.documentElement.dataset.theme`; system preference changes are observed only while the preference is `system`.

A small script in `web/index.html` applies the stored or system-resolved theme before the app module loads, updates `color-scheme`, and keeps the browser theme-color aligned. The Solid theme controller owns subsequent updates. The masthead exposes an accessible native select for the three choices.

Semantic theme tokens allow the same Tailwind classes to work in both themes. The dark palette remains restrained and editorial: near-black green canvas, lifted green-neutral surfaces, warm off-white text, pale moss accent, and coral for destructive/error states.

## Accessibility and responsive behavior

All existing semantics, labels, focus behavior, and keyboard navigation remain intact. Controls provide visible focus and disabled states. Text contrast targets WCAG AA in both themes. Browser verification covers 320, 768, 1024, and 1440 pixel widths.

## Verification

- Theme resolution and persistence unit tests
- Theme selector interaction test
- Existing Vitest suite
- TypeScript typecheck
- Production build
- Mechanical check that component/page selectors no longer remain in `styles.css`
- Browser verification in light, dark, and system modes with a clean console
