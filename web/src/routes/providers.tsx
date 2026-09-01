import { useMutation, useQuery } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { ProviderStatus } from "../components/provider-status";
import { RequestError } from "../components/request-error";
import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { PageHeader } from "../components/ui/page-header";
import { SectionHeader } from "../components/ui/section-header";
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
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Providers / Connectivity"
        title="Provider readiness"
        description="Verify one source at a time. Results expose actionable categories, never credentials or endpoint details."
      />

      <section class="mt-12" aria-labelledby="provider-ledger-title">
        <SectionHeader
          class="pb-6"
          eyebrow="Registered sources"
          title="Connectivity ledger"
          titleId="provider-ledger-title"
          meta={
            <CountBadge>
              {catalog.data?.providers.length ?? 0} providers
            </CountBadge>
          }
        />
        <Show when={catalog.isPending || settings.isPending}>
          <LoadingState>Loading provider configuration…</LoadingState>
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
          <EmptyState
            title="No providers registered"
            description="The server did not advertise any metadata sources."
          />
        </Show>
        <Show when={catalog.isSuccess && settings.isSuccess}>
          <div class="border-t-2 border-ink">
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
