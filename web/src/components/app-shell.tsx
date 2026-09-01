import { Link, Outlet } from "@tanstack/solid-router";
import type { JSX } from "@solidjs/web";
import { createSignal, onCleanup } from "solid-js";

import { createThemeController, type ThemePreference } from "../lib/theme";
import { ThemeSelect } from "./ui/theme-select";

const navigation = [
  { to: "/", label: "Workspace", marker: "01" },
  { to: "/jobs", label: "Jobs", marker: "02" },
  { to: "/search", label: "Search", marker: "03" },
  { to: "/library", label: "Library", marker: "04" },
  { to: "/providers", label: "Providers", marker: "05" },
  { to: "/settings", label: "Settings", marker: "06" },
  { to: "/templates", label: "Templates", marker: "07" },
] as const;

export function AppShell(): JSX.Element {
  let main!: HTMLElement;
  const [themePreference, setThemePreference] = createSignal<ThemePreference>(
    "system",
    { ownedWrite: true },
  );
  const themeController = createThemeController(({ preference }) =>
    setThemePreference(preference),
  );
  onCleanup(() => themeController.dispose());

  const focusWorkspace: JSX.EventHandlerUnion<HTMLAnchorElement, MouseEvent> = (
    event,
  ) => {
    event.preventDefault();
    main.focus();
  };

  return (
    <div class="min-h-screen bg-paper text-ink">
      <a
        class="fixed top-4 left-4 z-20 -translate-y-[200%] bg-ink px-4 py-3 text-paper transition-transform focus:translate-y-0"
        href="#workspace"
        onClick={focusWorkspace}
      >
        Skip to workspace
      </a>
      <header class="flex h-[86px] items-center justify-between border-b border-line px-[clamp(1rem,4vw,4rem)] max-[480px]:h-[72px] max-[480px]:px-4">
        <Link
          class="flex items-center gap-3 no-underline"
          to="/"
          aria-label="Fixer workspace home"
        >
          <span
            class="grid size-[38px] place-items-center rounded-full bg-moss font-serif text-xl font-bold text-paper"
            aria-hidden="true"
          >
            F
          </span>
          <span>
            <strong class="block font-serif text-lg font-bold">Fixer</strong>
            <small class="block text-[0.68rem] uppercase tracking-[0.08em] text-muted">
              Metadata operations
            </small>
          </span>
        </Link>
        <div class="flex items-center gap-4">
          <div
            class="text-[0.78rem] tracking-[0.04em] text-muted max-[480px]:hidden"
            aria-label="Current environment"
          >
            <span
              class="mr-2 inline-block size-[7px] rounded-full bg-success"
              aria-hidden="true"
            />
            Local workspace
          </div>
          <ThemeSelect
            value={themePreference()}
            onChange={(preference) =>
              themeController.setPreference(preference)
            }
          />
        </div>
      </header>
      <div class="grid min-h-[calc(100vh-86px)] grid-cols-[230px_minmax(0,1fr)] max-[800px]:min-h-[calc(100vh-86px)] max-[800px]:grid-cols-1 max-[480px]:min-h-[calc(100vh-72px)]">
        <aside
          class="flex flex-col border-r border-line px-6 pt-12 pb-8 max-[800px]:border-r-0 max-[800px]:border-b max-[800px]:p-4"
          aria-label="Workspace navigation"
        >
          <p class="mb-4 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted max-[800px]:hidden">
            Navigate
          </p>
          <nav class="max-[800px]:overflow-x-auto">
            <ul class="m-0 list-none p-0 max-[800px]:flex max-[800px]:w-max max-[800px]:gap-4">
              {navigation.map((item) => (
                <li>
                  <Link
                    to={item.to}
                    activeOptions={{ exact: item.to === "/" }}
                    activeProps={{
                      "aria-current": "page",
                      class:
                        "flex gap-3 border-b border-ink px-3 py-3 font-semibold text-ink no-underline max-[800px]:px-2 max-[800px]:py-2 max-[800px]:whitespace-nowrap",
                    }}
                    inactiveProps={{
                      class:
                        "flex gap-3 border-b border-transparent px-3 py-3 text-muted no-underline transition-colors hover:text-moss max-[800px]:px-2 max-[800px]:py-2 max-[800px]:whitespace-nowrap",
                    }}
                  >
                    <span
                      class="pt-1 text-[0.65rem] text-muted"
                      aria-hidden="true"
                    >
                      {item.marker}
                    </span>
                    {item.label}
                  </Link>
                </li>
              ))}
            </ul>
          </nav>
          <div class="mt-auto flex items-start border-t border-line pt-5 text-xs text-muted max-[800px]:hidden">
            <span
              class="mt-1 mr-2 inline-block size-[7px] shrink-0 rounded-full bg-success"
              aria-hidden="true"
            />
            <p class="m-0">
              <strong class="text-ink">Local first</strong>
              <br />
              Your media stays on this machine.
            </p>
          </div>
        </aside>
        <main
          id="workspace"
          ref={main}
          tabindex="-1"
          class="min-w-0 p-[clamp(2rem,6vw,6rem)] focus:outline-none max-[800px]:px-4 max-[800px]:pt-8 max-[800px]:pb-16"
        >
          <Outlet />
        </main>
      </div>
    </div>
  );
}
