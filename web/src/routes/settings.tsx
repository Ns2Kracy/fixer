import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { For, Show, createEffect, createSignal } from "solid-js";

import { LocalePolicyEditor } from "../components/locale-policy-editor";
import { RequestError } from "../components/request-error";
import { Button } from "../components/ui/button";
import { FormField } from "../components/ui/form-field";
import { LoadingState } from "../components/ui/loading-state";
import { PageHeader } from "../components/ui/page-header";
import {
  api,
  isConflictPolicy,
  isOutputPreset,
  isPlacementPolicy,
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
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Settings / Scraper policy"
        title="Workspace settings"
        description="Tune evidence, transport, output, and provider policy. Secret values are accepted once and never read back."
      />

      <Show when={settings.isPending}>
        <LoadingState>Loading workspace policy…</LoadingState>
      </Show>
      <Show when={settings.isError}>
        <RequestError error={settings.error} />
      </Show>
      <Show when={draft()}>
        {(form) => (
          <form class="mt-12" onSubmit={submit}>
            <fieldset
              class="m-0 min-w-0 border-0 p-0"
              disabled={update.isPending}
              aria-label="Workspace settings fields"
            >
              <section
                class="grid grid-cols-[minmax(210px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-t-2 border-ink py-10 pb-16 max-[1000px]:grid-cols-1"
                aria-labelledby="behavior-settings-title"
              >
                <div>
                  <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                    01 / Resolution
                  </p>
                  <h2
                    class="m-0 font-serif text-2xl font-medium"
                    id="behavior-settings-title"
                  >
                    Behavior policy
                  </h2>
                  <p class="mt-3 mb-0 text-sm text-muted">
                    Control locale priority, confidence gates, and offline
                    operation.
                  </p>
                </div>
                <div class="grid grid-cols-2 gap-5 max-[700px]:grid-cols-1">
                  <LocalePolicyEditor
                    value={form().preferred_locales}
                    disabled={update.isPending}
                    onChange={(locales) => {
                      patch("preferred_locales", locales);
                    }}
                  />
                  <FormField
                    label="Proxy URL"
                    hint="Credential-free HTTP, HTTPS, or SOCKS proxy."
                  >
                    <input
                      type="url"
                      value={form().proxy ?? ""}
                      placeholder="socks5://127.0.0.1:1080"
                      onInput={(event) => {
                        patch("proxy", event.currentTarget.value || null);
                      }}
                    />
                  </FormField>
                  <FormField label="Request timeout (seconds)">
                    <input
                      type="number"
                      min="1"
                      max="300"
                      required
                      value={form().timeout_seconds}
                      onInput={(event) => {
                        patch(
                          "timeout_seconds",
                          Number(event.currentTarget.value),
                        );
                      }}
                    />
                  </FormField>
                  <FormField label="Auto-accept confidence">
                    <input
                      type="number"
                      min="0"
                      max="1"
                      step="0.01"
                      required
                      value={form().auto_accept_confidence}
                      onInput={(event) => {
                        patch(
                          "auto_accept_confidence",
                          Number(event.currentTarget.value),
                        );
                      }}
                    />
                  </FormField>
                  <FormField label="Review confidence">
                    <input
                      type="number"
                      min="0"
                      max="1"
                      step="0.01"
                      required
                      value={form().review_confidence}
                      onInput={(event) => {
                        patch(
                          "review_confidence",
                          Number(event.currentTarget.value),
                        );
                      }}
                    />
                  </FormField>
                  <label
                    class="flex items-start gap-3 pt-6 text-sm text-muted"
                    aria-label="Offline mode"
                  >
                    <input
                      class="mt-[0.15rem] size-[1.05rem] shrink-0 accent-moss"
                      type="checkbox"
                      checked={form().offline}
                      onChange={(event) => {
                        patch("offline", event.currentTarget.checked);
                      }}
                    />
                    <span>
                      <strong class="block text-ink">Offline mode</strong>
                      <small class="mt-1 block leading-relaxed text-muted">
                        Skip every network provider before transport is invoked.
                      </small>
                    </span>
                  </label>
                </div>
              </section>

              <section
                class="grid grid-cols-[minmax(210px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-t border-line py-10 pb-16 max-[1000px]:grid-cols-1"
                aria-labelledby="output-settings-title"
              >
                <div>
                  <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                    02 / Output
                  </p>
                  <h2
                    class="m-0 font-serif text-2xl font-medium"
                    id="output-settings-title"
                  >
                    Plan defaults
                  </h2>
                  <p class="mt-3 mb-0 text-sm text-muted">
                    Choose what is planned and how media placement conflicts are
                    handled.
                  </p>
                </div>
                <div class="grid grid-cols-3 gap-5 max-[800px]:grid-cols-1">
                  <FormField label="Output preset">
                    <select
                      value={form().output_preset}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        if (isOutputPreset(value))
                          patch("output_preset", value);
                      }}
                    >
                      <option value="full">Full media package</option>
                      <option value="metadata">Metadata only</option>
                    </select>
                  </FormField>
                  <FormField label="Placement">
                    <select
                      value={form().placement}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        if (isPlacementPolicy(value)) patch("placement", value);
                      }}
                    >
                      <option value="in_place">In place</option>
                      <option value="symlink">Symlink</option>
                      <option value="hardlink">Hardlink</option>
                      <option value="copy">Copy</option>
                      <option value="reflink">Reflink</option>
                    </select>
                  </FormField>
                  <FormField label="Conflict policy">
                    <select
                      value={form().conflict_policy}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        if (isConflictPolicy(value))
                          patch("conflict_policy", value);
                      }}
                    >
                      <option value="prefer_first">Prefer first source</option>
                      <option value="review">Require review</option>
                      <option value="error">Stop on conflict</option>
                    </select>
                  </FormField>
                </div>
              </section>

              <section
                class="grid grid-cols-[minmax(210px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-t border-line py-10 pb-16 max-[1000px]:grid-cols-1"
                aria-labelledby="provider-settings-title"
              >
                <div>
                  <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                    03 / Providers
                  </p>
                  <h2
                    class="m-0 font-serif text-2xl font-medium"
                    id="provider-settings-title"
                  >
                    Source registry
                  </h2>
                  <p class="mt-3 mb-0 text-sm text-muted">
                    Enable sources and override only their documented base
                    endpoints.
                  </p>
                </div>
                <div class="grid gap-5">
                  <fieldset class="m-0 grid grid-cols-3 gap-3 border-0 p-0 max-[700px]:grid-cols-2">
                    <legend class="mb-4 w-full border-b border-line pb-3 font-serif text-lg font-medium">
                      Enabled providers
                    </legend>
                    <For each={providerOptions}>
                      {(provider) => (
                        <label class="flex items-center gap-2 text-sm text-muted">
                          <input
                            class="size-[1.05rem] accent-moss"
                            type="checkbox"
                            checked={form().enabled_providers.includes(
                              provider.id,
                            )}
                            onChange={(event) => {
                              toggleProvider(
                                provider.id,
                                event.currentTarget.checked,
                              );
                            }}
                          />
                          <span>{provider.label}</span>
                        </label>
                      )}
                    </For>
                  </fieldset>
                  <div class="mt-6 grid grid-cols-2 gap-4 max-[700px]:grid-cols-1">
                    <For each={endpointOptions}>
                      {(endpoint) => (
                        <FormField label={endpoint.label}>
                          <input
                            type="url"
                            required
                            value={form().provider_endpoints[endpoint.key]}
                            onInput={(event) => {
                              patchEndpoint(
                                endpoint.key,
                                event.currentTarget.value,
                              );
                            }}
                          />
                        </FormField>
                      )}
                    </For>
                  </div>
                </div>
              </section>

              <section
                class="grid grid-cols-[minmax(210px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-t border-line py-10 pb-16 max-[1000px]:grid-cols-1"
                aria-labelledby="secret-settings-title"
              >
                <div>
                  <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                    04 / Credentials
                  </p>
                  <h2
                    class="m-0 font-serif text-2xl font-medium"
                    id="secret-settings-title"
                  >
                    Write-only secrets
                  </h2>
                  <p class="mt-3 mb-0 text-sm text-muted">
                    Blank fields preserve configured values. Clear explicitly
                    when revoking access.
                  </p>
                </div>
                <div class="grid grid-cols-2 gap-5 max-[700px]:grid-cols-1">
                  <SecretField
                    label="TMDB API token"
                    configured={
                      settings.data?.settings.secrets
                        .tmdb_api_token_configured ?? false
                    }
                    value={form().tmdb_api_token ?? ""}
                    clear={form().clear_tmdb_api_token}
                    onValue={(value) => {
                      setSecret(
                        "tmdb_api_token",
                        "clear_tmdb_api_token",
                        value,
                      );
                    }}
                    onClear={(clear) => {
                      clearSecret(
                        "tmdb_api_token",
                        "clear_tmdb_api_token",
                        clear,
                      );
                    }}
                  />
                  <SecretField
                    label="AniList access token"
                    configured={
                      settings.data?.settings.secrets
                        .anilist_access_token_configured ?? false
                    }
                    value={form().anilist_access_token ?? ""}
                    clear={form().clear_anilist_access_token}
                    onValue={(value) => {
                      setSecret(
                        "anilist_access_token",
                        "clear_anilist_access_token",
                        value,
                      );
                    }}
                    onClear={(clear) => {
                      clearSecret(
                        "anilist_access_token",
                        "clear_anilist_access_token",
                        clear,
                      );
                    }}
                  />
                </div>
              </section>
            </fieldset>

            <div class="sticky bottom-4 z-4 flex items-center justify-between gap-8 border border-ink bg-overlay px-5 py-4 shadow-[0_10px_30px_var(--color-shadow)] max-[700px]:static max-[700px]:flex-col max-[700px]:items-stretch">
              <div aria-live="polite">
                <Show when={saved()}>
                  <p class="m-0 font-bold text-success" role="status">
                    Settings saved
                  </p>
                </Show>
                <Show when={update.isError}>
                  <RequestError error={update.error} />
                </Show>
              </div>
              <Button
                class="max-[700px]:w-full"
                type="submit"
                disabled={update.isPending}
              >
                {update.isPending ? "Saving…" : "Save settings"}
              </Button>
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
    <div class="border-t-2 border-ink pt-4">
      <label
        class="relative grid gap-2 text-sm font-medium text-muted"
        for={fieldId}
      >
        <span>{props.label}</span>
        <span
          class="absolute top-0 right-0 text-[0.62rem] text-success"
          id={stateId}
        >
          {props.configured ? "Configured" : "Not configured"}
        </span>
        <input
          class="min-h-11 border border-line bg-surface px-3 py-2.5 text-ink outline-none transition-colors focus-visible:border-coral"
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
          onInput={(event) => {
            props.onValue(event.currentTarget.value);
          }}
        />
      </label>
      <label class="flex items-start gap-3 pt-3 text-sm text-muted">
        <input
          class="mt-[0.15rem] size-[1.05rem] shrink-0 accent-moss"
          type="checkbox"
          aria-label={`Clear ${props.label}`}
          checked={props.clear}
          onChange={(event) => {
            props.onClear(event.currentTarget.checked);
          }}
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
