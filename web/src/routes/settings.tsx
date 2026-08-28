import { For, Show, createEffect, createSignal } from "solid-js";
import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";

import { LocalePolicyEditor } from "../components/locale-policy-editor";
import { RequestError } from "../components/request-error";
import {
  api,
  type ConflictPolicy,
  type PlacementPolicy,
  type ProviderEndpoints,
  type ProviderId,
  type UpdateWorkspaceSettingsRequest,
  type WorkspaceSettings,
} from "../lib/api";

export const Route = createFileRoute("/settings")({
  component: SettingsPage,
});

const providerOptions: Array<{ id: ProviderId; label: string }> = [
  { id: "local", label: "Local files" },
  { id: "tmdb", label: "TMDB" },
  { id: "bangumi", label: "Bangumi" },
  { id: "anilist", label: "AniList" },
  { id: "musicbrainz", label: "MusicBrainz" },
  { id: "openlibrary", label: "Open Library" },
];

const endpointOptions: Array<{ key: keyof ProviderEndpoints; label: string }> =
  [
    { key: "tmdb", label: "TMDB endpoint" },
    { key: "bangumi", label: "Bangumi endpoint" },
    { key: "anilist", label: "AniList endpoint" },
    { key: "musicbrainz", label: "MusicBrainz endpoint" },
    { key: "openlibrary", label: "Open Library endpoint" },
    { key: "openlibrary_cover", label: "Open Library cover endpoint" },
  ];

function SettingsPage() {
  const queryClient = useQueryClient();
  const [draft, setDraft] = createSignal<UpdateWorkspaceSettingsRequest>();
  const [saved, setSaved] = createSignal(false);
  const settings = useQuery(() => ({
    queryKey: ["settings"],
    queryFn: () => api.settings(),
  }));
  const update = useMutation(() => ({
    mutationFn: (request: UpdateWorkspaceSettingsRequest) =>
      api.updateSettings(request),
    onMutate: () => setSaved(false),
    onSuccess: (response) => {
      setDraft(editableSettings(response.settings));
      queryClient.setQueryData(["settings"], response);
      setSaved(true);
    },
  }));

  createEffect(
    () => {
      const snapshot = settings.data?.settings;
      return snapshot ? editableSettings(snapshot) : undefined;
    },
    (editable) => {
      if (editable) setDraft(editable);
    },
  );

  function patch<K extends keyof UpdateWorkspaceSettingsRequest>(
    key: K,
    value: UpdateWorkspaceSettingsRequest[K],
  ) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
    setSaved(false);
  }

  function patchEndpoint(key: keyof ProviderEndpoints, value: string) {
    setDraft((current) =>
      current
        ? {
            ...current,
            provider_endpoints: { ...current.provider_endpoints, [key]: value },
          }
        : current,
    );
    setSaved(false);
  }

  function toggleProvider(provider: ProviderId, enabled: boolean) {
    const current = draft();
    if (!current) return;
    const providers = new Set(current.enabled_providers);
    if (enabled) providers.add(provider);
    else providers.delete(provider);
    patch("enabled_providers", Array.from(providers));
  }

  function setSecret(
    field: "tmdb_api_token" | "anilist_access_token",
    clearField: "clear_tmdb_api_token" | "clear_anilist_access_token",
    value: string,
  ) {
    setDraft((current) =>
      current
        ? {
            ...current,
            [field]: value || null,
            ...(value ? { [clearField]: false } : {}),
          }
        : current,
    );
    setSaved(false);
  }

  function clearSecret(
    field: "tmdb_api_token" | "anilist_access_token",
    clearField: "clear_tmdb_api_token" | "clear_anilist_access_token",
    clear: boolean,
  ) {
    setDraft((current) =>
      current ? { ...current, [field]: null, [clearField]: clear } : current,
    );
    setSaved(false);
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const request = draft();
    if (request) update.mutate(request);
  }

  return (
    <div class="workspace-page settings-page">
      <header class="workspace-heading">
        <div>
          <p class="eyebrow">Settings / Scraper policy</p>
          <h1>Workspace settings</h1>
        </div>
        <p>
          Tune evidence, transport, output, and provider policy. Secret values
          are accepted once and never read back.
        </p>
      </header>

      <Show when={settings.isPending}>
        <p class="loading-line">Loading workspace policy…</p>
      </Show>
      <Show when={settings.isError}>
        <RequestError error={settings.error} />
      </Show>
      <Show when={draft()}>
        {(form) => (
          <form class="settings-form" onSubmit={submit}>
            <fieldset
              class="settings-form-fields"
              disabled={update.isPending}
              aria-label="Workspace settings fields"
            >
            <section
              class="settings-section"
              aria-labelledby="behavior-settings-title"
            >
              <div class="settings-section-intro">
                <p class="eyebrow">01 / Resolution</p>
                <h2 id="behavior-settings-title">Behavior policy</h2>
                <p>
                  Control locale priority, confidence gates, and offline
                  operation.
                </p>
              </div>
              <div class="settings-fields two-column">
                <LocalePolicyEditor
                  value={form().preferred_locales}
                  disabled={update.isPending}
                  onChange={(locales) => patch("preferred_locales", locales)}
                />
                <label>
                  <span>Proxy URL</span>
                  <input
                    type="url"
                    value={form().proxy ?? ""}
                    placeholder="socks5://127.0.0.1:1080"
                    onInput={(event) =>
                      patch("proxy", event.currentTarget.value || null)
                    }
                  />
                  <small>Credential-free HTTP, HTTPS, or SOCKS proxy.</small>
                </label>
                <label>
                  <span>Request timeout (seconds)</span>
                  <input
                    type="number"
                    min="1"
                    max="300"
                    required
                    value={form().timeout_seconds}
                    onInput={(event) =>
                      patch(
                        "timeout_seconds",
                        Number(event.currentTarget.value),
                      )
                    }
                  />
                </label>
                <label>
                  <span>Auto-accept confidence</span>
                  <input
                    type="number"
                    min="0"
                    max="1"
                    step="0.01"
                    required
                    value={form().auto_accept_confidence}
                    onInput={(event) =>
                      patch(
                        "auto_accept_confidence",
                        Number(event.currentTarget.value),
                      )
                    }
                  />
                </label>
                <label>
                  <span>Review confidence</span>
                  <input
                    type="number"
                    min="0"
                    max="1"
                    step="0.01"
                    required
                    value={form().review_confidence}
                    onInput={(event) =>
                      patch(
                        "review_confidence",
                        Number(event.currentTarget.value),
                      )
                    }
                  />
                </label>
                <label class="settings-check">
                  <input
                    type="checkbox"
                    checked={form().offline}
                    onChange={(event) =>
                      patch("offline", event.currentTarget.checked)
                    }
                  />
                  <span>
                    <strong>Offline mode</strong>
                    <small>
                      Skip every network provider before transport is invoked.
                    </small>
                  </span>
                </label>
              </div>
            </section>

            <section
              class="settings-section"
              aria-labelledby="output-settings-title"
            >
              <div class="settings-section-intro">
                <p class="eyebrow">02 / Output</p>
                <h2 id="output-settings-title">Plan defaults</h2>
                <p>
                  Choose what is planned and how media placement conflicts are
                  handled.
                </p>
              </div>
              <div class="settings-fields three-column">
                <label>
                  <span>Output preset</span>
                  <select
                    value={form().output_preset}
                    onChange={(event) =>
                      patch(
                        "output_preset",
                        event.currentTarget.value as "full" | "metadata",
                      )
                    }
                  >
                    <option value="full">Full media package</option>
                    <option value="metadata">Metadata only</option>
                  </select>
                </label>
                <label>
                  <span>Placement</span>
                  <select
                    value={form().placement}
                    onChange={(event) =>
                      patch(
                        "placement",
                        event.currentTarget.value as PlacementPolicy,
                      )
                    }
                  >
                    <option value="in_place">In place</option>
                    <option value="symlink">Symlink</option>
                    <option value="hardlink">Hardlink</option>
                    <option value="copy">Copy</option>
                    <option value="reflink">Reflink</option>
                  </select>
                </label>
                <label>
                  <span>Conflict policy</span>
                  <select
                    value={form().conflict_policy}
                    onChange={(event) =>
                      patch(
                        "conflict_policy",
                        event.currentTarget.value as ConflictPolicy,
                      )
                    }
                  >
                    <option value="prefer_first">Prefer first source</option>
                    <option value="review">Require review</option>
                    <option value="error">Stop on conflict</option>
                  </select>
                </label>
              </div>
            </section>

            <section
              class="settings-section"
              aria-labelledby="provider-settings-title"
            >
              <div class="settings-section-intro">
                <p class="eyebrow">03 / Providers</p>
                <h2 id="provider-settings-title">Source registry</h2>
                <p>
                  Enable sources and override only their documented base
                  endpoints.
                </p>
              </div>
              <div class="settings-fields">
                <fieldset class="provider-toggles">
                  <legend>Enabled providers</legend>
                  <For each={providerOptions}>
                    {(provider) => (
                      <label>
                        <input
                          type="checkbox"
                          checked={form().enabled_providers.includes(
                            provider.id,
                          )}
                          onChange={(event) =>
                            toggleProvider(
                              provider.id,
                              event.currentTarget.checked,
                            )
                          }
                        />
                        <span>{provider.label}</span>
                      </label>
                    )}
                  </For>
                </fieldset>
                <div class="endpoint-fields">
                  <For each={endpointOptions}>
                    {(endpoint) => (
                      <label>
                        <span>{endpoint.label}</span>
                        <input
                          type="url"
                          required
                          value={form().provider_endpoints[endpoint.key]}
                          onInput={(event) =>
                            patchEndpoint(
                              endpoint.key,
                              event.currentTarget.value,
                            )
                          }
                        />
                      </label>
                    )}
                  </For>
                </div>
              </div>
            </section>

            <section
              class="settings-section"
              aria-labelledby="secret-settings-title"
            >
              <div class="settings-section-intro">
                <p class="eyebrow">04 / Credentials</p>
                <h2 id="secret-settings-title">Write-only secrets</h2>
                <p>
                  Blank fields preserve configured values. Clear explicitly when
                  revoking access.
                </p>
              </div>
              <div class="settings-fields secret-fields">
                <SecretField
                  label="TMDB API token"
                  configured={
                    settings.data?.settings.secrets.tmdb_api_token_configured ??
                    false
                  }
                  value={form().tmdb_api_token ?? ""}
                  clear={form().clear_tmdb_api_token}
                  onValue={(value) =>
                    setSecret("tmdb_api_token", "clear_tmdb_api_token", value)
                  }
                  onClear={(clear) =>
                    clearSecret("tmdb_api_token", "clear_tmdb_api_token", clear)
                  }
                />
                <SecretField
                  label="AniList access token"
                  configured={
                    settings.data?.settings.secrets
                      .anilist_access_token_configured ?? false
                  }
                  value={form().anilist_access_token ?? ""}
                  clear={form().clear_anilist_access_token}
                  onValue={(value) =>
                    setSecret(
                      "anilist_access_token",
                      "clear_anilist_access_token",
                      value,
                    )
                  }
                  onClear={(clear) =>
                    clearSecret(
                      "anilist_access_token",
                      "clear_anilist_access_token",
                      clear,
                    )
                  }
                />
              </div>
            </section>

            </fieldset>

            <div class="settings-save-bar">
              <div aria-live="polite">
                <Show when={saved()}>
                  <p class="save-state" role="status">
                    Settings saved
                  </p>
                </Show>
                <Show when={update.isError}>
                  <RequestError error={update.error} />
                </Show>
              </div>
              <button
                class="button primary"
                type="submit"
                disabled={update.isPending}
              >
                {update.isPending ? "Saving…" : "Save settings"}
              </button>
            </div>
          </form>
        )}
      </Show>
    </div>
  );
}

interface SecretFieldProps {
  label: string;
  configured: boolean;
  value: string;
  clear: boolean;
  onValue: (value: string) => void;
  onClear: (clear: boolean) => void;
}

function SecretField(props: SecretFieldProps) {
  const fieldId = `${props.label.toLowerCase().replaceAll(" ", "-")}-field`;
  const stateId = `${fieldId}-state`;

  return (
    <div class="secret-field">
      <label for={fieldId}>
        <span>{props.label}</span>
        <span class="secret-state" id={stateId}>
          {props.configured ? "Configured" : "Not configured"}
        </span>
        <input
          id={fieldId}
          type="password"
          aria-label={props.label}
          aria-describedby={stateId}
          value={props.value}
          autocomplete="new-password"
          placeholder={
            props.configured ? "Leave blank to preserve" : "Enter token"
          }
          disabled={props.clear}
          onInput={(event) => props.onValue(event.currentTarget.value)}
        />
      </label>
      <label class="settings-check clear-secret">
        <input
          type="checkbox"
          aria-label={`Clear ${props.label}`}
          checked={props.clear}
          onChange={(event) => props.onClear(event.currentTarget.checked)}
        />
        <span>Clear configured secret</span>
      </label>
    </div>
  );
}

function editableSettings(
  settings: WorkspaceSettings,
): UpdateWorkspaceSettingsRequest {
  return {
    offline: settings.offline,
    proxy: settings.proxy,
    preferred_locales: [...settings.preferred_locales],
    timeout_seconds: settings.timeout_seconds,
    auto_accept_confidence: settings.auto_accept_confidence,
    review_confidence: settings.review_confidence,
    output_preset: settings.output_preset,
    placement: settings.placement,
    conflict_policy: settings.conflict_policy,
    enabled_providers: [...settings.enabled_providers],
    provider_endpoints: { ...settings.provider_endpoints },
    tmdb_api_token: null,
    anilist_access_token: null,
    clear_tmdb_api_token: false,
    clear_anilist_access_token: false,
  };
}
