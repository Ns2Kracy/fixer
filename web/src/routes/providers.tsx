import { For, Show, createSignal } from "solid-js";
import { useMutation, useQuery } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";

import { ProviderStatus } from "../components/provider-status";
import { RequestError } from "../components/request-error";
import { api, type ProviderId, type ProviderProbeEnvelope } from "../lib/api";

export const Route = createFileRoute("/providers")({
  component: ProvidersPage,
});

function ProvidersPage() {
  const [testingId, setTestingId] = createSignal<ProviderId>();
  const [results, setResults] = createSignal<
    Partial<Record<ProviderId, ProviderProbeEnvelope>>
  >({});
  const catalog = useQuery(() => ({
    queryKey: ["providers"],
    queryFn: () => api.providers(),
  }));
  const settings = useQuery(() => ({
    queryKey: ["settings"],
    queryFn: () => api.settings(),
  }));
  const probe = useMutation(() => ({
    mutationFn: (provider: ProviderId) => api.testProvider(provider),
    onMutate: (provider) => setTestingId(provider),
    onSuccess: (result) => {
      setResults((current) => ({ ...current, [result.provider]: result }));
    },
    onSettled: () => setTestingId(undefined),
  }));

  function isEnabled(provider: string) {
    return (
      settings.data?.settings.enabled_providers.includes(
        provider as ProviderId,
      ) ?? false
    );
  }

  return (
    <div class="workspace-page providers-page">
      <header class="workspace-heading">
        <div>
          <p class="eyebrow">Providers / Connectivity</p>
          <h1>Provider readiness</h1>
        </div>
        <p>
          Verify one source at a time. Results expose actionable categories,
          never credentials or endpoint details.
        </p>
      </header>

      <section class="provider-ledger" aria-labelledby="provider-ledger-title">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Registered sources</p>
            <h2 id="provider-ledger-title">Connectivity ledger</h2>
          </div>
          <span class="count">
            {catalog.data?.providers.length ?? 0} providers
          </span>
        </div>
        <Show when={catalog.isPending || settings.isPending}>
          <p class="loading-line">Loading provider configuration…</p>
        </Show>
        <Show when={catalog.isError}>
          <RequestError error={catalog.error} />
        </Show>
        <Show when={settings.isError}>
          <RequestError error={settings.error} />
        </Show>
        <Show when={probe.isError}>
          <RequestError error={probe.error} />
        </Show>
        <Show when={catalog.isSuccess && catalog.data?.providers.length === 0}>
          <div class="empty-inline">
            <div>
              <h3>No providers registered</h3>
              <p>The server did not advertise any metadata sources.</p>
            </div>
          </div>
        </Show>
        <Show when={catalog.isSuccess && settings.isSuccess}>
          <div class="provider-rows">
            <For each={catalog.data?.providers ?? []}>
              {(provider) => (
                <ProviderStatus
                  provider={provider}
                  enabled={isEnabled(provider.id)}
                  testing={probe.isPending && testingId() === provider.id}
                  disabled={probe.isPending}
                  result={results()[provider.id as ProviderId]}
                  onTest={() => probe.mutate(provider.id as ProviderId)}
                />
              )}
            </For>
          </div>
        </Show>
      </section>
    </div>
  );
}
