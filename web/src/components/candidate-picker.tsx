import { For, Show } from 'solid-js'

import type { CandidateArtifact } from '../lib/api'

export function CandidatePicker(props: {
  candidates: CandidateArtifact[]
  selectedIndex: number
  onSelect: (index: number) => void
}) {
  return (
    <fieldset class="candidate-picker">
      <legend>Candidate matches</legend>
      <For each={props.candidates}>
        {(candidate) => (
          <label
            class={`candidate-option${candidate.index === props.selectedIndex ? ' selected' : ''}`}
          >
            <input
              type="radio"
              name="candidate"
              value={candidate.index}
              checked={candidate.index === props.selectedIndex}
              aria-label={`Select ${candidate.title} from ${candidate.provider}`}
              onChange={() => props.onSelect(candidate.index)}
            />
            <span class="candidate-main">
              <span class="candidate-title">
                <strong>{candidate.title}</strong>
                <Show when={candidate.year}>{(year) => <span>{year()}</span>}</Show>
              </span>
              <span class="candidate-provider">
                {candidate.provider} · {candidate.external_id.namespace}:{candidate.external_id.value}
              </span>
              <ul class="evidence-list" aria-label={`Evidence for ${candidate.title}`}>
                <For each={candidate.evidence}>
                  {(evidence) => (
                    <li>
                      <span class={evidence.points >= 0 ? 'positive' : 'negative'}>
                        {evidence.points >= 0 ? '+' : ''}{evidence.points}
                      </span>
                      {evidence.detail}
                    </li>
                  )}
                </For>
              </ul>
              <Show when={candidate.evidence_truncated}>
                <span class="truncation-inline">Additional matching evidence was omitted.</span>
              </Show>
            </span>
            <span class="candidate-score" aria-label={`Score ${candidate.score}`}>
              {candidate.score}
            </span>
          </label>
        )}
      </For>
    </fieldset>
  )
}
