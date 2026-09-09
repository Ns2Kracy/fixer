import { useMutation, useQueryClient } from "@tanstack/solid-query";
import { Link, Outlet, useLocation, useNavigate } from "@tanstack/solid-router";
import type { JSX } from "@solidjs/web";
import { For, Show, createMemo, createSignal, onCleanup } from "solid-js";

import { api } from "../lib/api";
import { createThemeController, type ThemePreference } from "../lib/theme";
import { Button } from "./ui/button";
import { ThemeSelect } from "./ui/theme-select";

const navigation = [
  { to: "/", label: "刮削" },
  { to: "/scrapes", label: "刮削审计" },
  { to: "/settings", label: "设置" },
] as const;
const settingsNavigation = [
  { to: "/settings", label: "通用与刮削设置" },
  { to: "/folders", label: "自动整理目录" },
  { to: "/providers", label: "数据源状态" },
  { to: "/templates", label: "命名规则" },
  { to: "/library", label: "浏览文件" },
] as const;

export function AppShell(): JSX.Element {
  let main: HTMLElement | undefined;
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const activePrimaryTarget = createMemo(() => {
    const path = location().pathname;
    if (path === "/") return "/";
    if (settingsNavigation.some((item) => item.to === path)) return "/settings";
    if (path === "/scrapes" || path.startsWith("/scrapes/")) return "/scrapes";
    return null;
  });
  const [themePreference, setThemePreference] = createSignal<ThemePreference>(
    "system",
    { ownedWrite: true },
  );
  const themeController = createThemeController(({ preference }) =>
    setThemePreference(preference),
  );
  onCleanup(() => {
    themeController.dispose();
  });
  const logout = useMutation(() => ({
    mutationFn: () => api.logout(),
    onSuccess: async () => {
      queryClient.setQueryData(["auth", "status"], {
        schema_version: 1,
        registration_required: false,
        authenticated: false,
        username: null,
      });
      await navigate({ to: "/login", replace: true });
    },
  }));

  const focusContent: JSX.EventHandlerUnion<HTMLAnchorElement, MouseEvent> = (
    event,
  ) => {
    event.preventDefault();
    main?.focus();
  };

  return (
    <div class="min-h-screen bg-paper text-ink">
      <a
        class="fixed top-4 left-4 z-20 -translate-y-[200%] bg-ink px-4 py-3 text-paper transition-transform focus:translate-y-0"
        href="#content"
        onClick={focusContent}
      >
        Skip to content
      </a>
      <header class="flex h-[86px] items-center justify-between border-b border-line px-[clamp(1rem,4vw,4rem)] max-[480px]:h-[72px] max-[480px]:px-4">
        <Link
          class="flex items-center gap-3 no-underline"
          to="/"
          aria-label="Fixer home"
        >
          <span
            class="grid size-[38px] place-items-center rounded-full bg-moss font-serif text-xl font-bold text-paper"
            aria-hidden="true"
          >
            F
          </span>
          <span class="max-[480px]:hidden">
            <strong class="block font-serif text-lg font-bold">Fixer</strong>
            <small class="block text-[0.68rem] uppercase tracking-[0.08em] text-muted">
              识别、刮削与文件整理
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
            Local service
          </div>
          <ThemeSelect
            value={themePreference()}
            onChange={(preference) => {
              themeController.setPreference(preference);
            }}
          />
          <Button
            class="min-h-9 px-3 py-2 text-sm"
            type="button"
            variant="secondary"
            disabled={logout.isPending}
            onClick={() => {
              logout.mutate();
            }}
          >
            {logout.isPending ? "Signing out…" : "Sign out"}
          </Button>
        </div>
      </header>
      <div class="grid min-h-[calc(100vh-86px)] grid-cols-[230px_minmax(0,1fr)] max-[800px]:min-h-[calc(100vh-86px)] max-[800px]:grid-cols-1 max-[480px]:min-h-[calc(100vh-72px)]">
        <aside
          class="flex flex-col border-r border-line px-6 pt-12 pb-8 max-[800px]:border-r-0 max-[800px]:border-b max-[800px]:p-4"
          aria-label="Primary navigation"
        >
          <p class="mb-4 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted max-[800px]:hidden">
            工作区
          </p>
          <nav aria-label="主导航" class="max-[800px]:overflow-x-auto">
            <ul class="m-0 list-none p-0 max-[800px]:flex max-[800px]:w-max max-[800px]:gap-4">
              <For each={navigation}>
                {(item) => (
                  <li>
                    <a
                      href={item.to}
                      aria-current={
                        activePrimaryTarget() === item.to ? "page" : undefined
                      }
                      class={
                        activePrimaryTarget() === item.to
                          ? "flex gap-3 border-b border-ink px-3 py-3 font-semibold text-ink no-underline max-[800px]:px-2 max-[800px]:py-2 max-[800px]:whitespace-nowrap"
                          : "flex gap-3 border-b border-transparent px-3 py-3 text-muted no-underline transition-colors hover:text-moss max-[800px]:px-2 max-[800px]:py-2 max-[800px]:whitespace-nowrap"
                      }
                      onClick={(event) => {
                        if (
                          event.button !== 0 ||
                          event.metaKey ||
                          event.ctrlKey ||
                          event.shiftKey ||
                          event.altKey
                        ) {
                          return;
                        }
                        event.preventDefault();
                        void navigate({ to: item.to });
                      }}
                    >
                      {item.label}
                    </a>
                  </li>
                )}
              </For>
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
          id="content"
          ref={(element) => {
            main = element;
          }}
          tabindex="-1"
          class="min-w-0 p-8 focus:outline-none max-[800px]:px-4 max-[800px]:pt-6 max-[800px]:pb-16"
        >
          <Show
            when={settingsNavigation.some(
              (item) => item.to === location().pathname,
            )}
          >
            <nav
              aria-label="设置分类"
              class="mb-8 flex flex-wrap gap-5 border-b border-line pb-4 text-sm"
            >
              {settingsNavigation.map((item) => (
                <Link
                  to={item.to}
                  activeProps={{
                    class: "font-bold text-ink",
                    "aria-current": "page",
                  }}
                  inactiveProps={{ class: "text-muted hover:text-ink" }}
                >
                  {item.label}
                </Link>
              ))}
            </nav>
          </Show>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
