import { Link, Outlet } from "@tanstack/solid-router";
import type { JSX } from "@solidjs/web";

const navigation = [
  { to: "/", label: "Workspace", marker: "01" },
  { to: "/jobs", label: "Jobs", marker: "02" },
  { to: "/search", label: "Search", marker: "03" },
  { to: "/library", label: "Library", marker: "04" },
  { to: "/providers", label: "Providers", marker: "05" },
  { to: "/settings", label: "Settings", marker: "06" },
  { to: "/templates", label: "Templates", marker: "07" },
  { to: "/login", label: "Sign in", marker: "08" },
] as const;

export function AppShell(): JSX.Element {
  let main!: HTMLElement;

  const focusWorkspace: JSX.EventHandlerUnion<HTMLAnchorElement, MouseEvent> = (
    event,
  ) => {
    event.preventDefault();
    main.focus();
  };

  return (
    <div class="app-frame">
      <a class="skip-link" href="#workspace" onClick={focusWorkspace}>
        Skip to workspace
      </a>
      <header class="masthead">
        <Link class="brand" to="/" aria-label="Fixer workspace home">
          <span class="brand-mark" aria-hidden="true">
            F
          </span>
          <span>
            <strong>Fixer</strong>
            <small>Metadata operations</small>
          </span>
        </Link>
        <div class="environment" aria-label="Current environment">
          <span aria-hidden="true" /> Local workspace
        </div>
      </header>
      <div class="workspace-grid">
        <aside class="rail" aria-label="Workspace navigation">
          <p class="eyebrow">Navigate</p>
          <nav>
            <ul>
              {navigation.map((item) => (
                <li>
                  <Link
                    to={item.to}
                    activeOptions={{ exact: item.to === "/" }}
                    activeProps={{
                      "aria-current": "page",
                      class: "nav-link active",
                    }}
                    inactiveProps={{ class: "nav-link" }}
                  >
                    <span aria-hidden="true">{item.marker}</span>
                    {item.label}
                  </Link>
                </li>
              ))}
            </ul>
          </nav>
          <div class="rail-note">
            <span class="status-dot" aria-hidden="true" />
            <p>
              <strong>Local first</strong>
              <br />
              Your media stays on this machine.
            </p>
          </div>
        </aside>
        <main id="workspace" ref={main} tabindex="-1">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
