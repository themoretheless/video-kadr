<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { captionCueDraft, sameCaptionCueDraft, type CaptionCueDraft } from '$lib/composition/captionTracks'
  import type { CaptionCue } from '$lib/composition/types'
  import { nestedHistory, updateNestedCaptionCue } from '$lib/state/nestedComposition.svelte'

  let { compositionId, trackId, cue, disabled = false }: {
    compositionId: string; trackId: string; cue: CaptionCue; disabled?: boolean
  } = $props()

  let draft = $state<CaptionCueDraft>({ text: '', speaker: '', cueId: '', settings: '', start: '', end: '' })
  let base = $state<CaptionCueDraft>({ text: '', speaker: '', cueId: '', settings: '', start: '', end: '' })
  let error = $state('')
  let disposed = false
  const changed = $derived(!sameCaptionCueDraft(draft, base))
  const conflict = $derived(!sameCaptionCueDraft(base, captionCueDraft(cue)))

  $effect(() => () => { disposed = true })
  $effect(() => {
    const next = captionCueDraft(cue)
    untrack(() => { if (!changed) adopt(next) })
  })

  function adopt(next: CaptionCueDraft) {
    base = next
    draft = { ...next }
    error = ''
  }

  async function save() {
    const composition = compositionId, track = trackId, id = cue.id
    const result = await updateNestedCaptionCue(composition, track, id, { ...draft }, { ...base }, () => {
      if (disposed || compositionId !== composition || trackId !== track || cue.id !== id || disabled) {
        throw new Error('Выбранная реплика изменилась')
      }
    })
    if (disposed) return
    if (result) {
      await tick()
      if (!disposed) adopt(captionCueDraft(cue))
    } else {
      error = nestedHistory.error || 'Сохранение отменено'
    }
  }
</script>

<fieldset aria-label="Редактировать реплику дорожки" disabled={disabled || nestedHistory.busy}>
  <legend>Реплика дорожки субтитров</legend>
  <label>Текст<textarea bind:value={draft.text}></textarea></label>
  <label>Голос<input type="text" bind:value={draft.speaker} /></label>
  <label>Идентификатор VTT<input type="text" bind:value={draft.cueId} /></label>
  <label>Параметры VTT<input type="text" bind:value={draft.settings} /></label>
  <label>Начало, с<input type="text" inputmode="decimal" bind:value={draft.start} /></label>
  <label>Конец, с<input type="text" inputmode="decimal" bind:value={draft.end} /></label>
  {#if conflict}<p role="alert">Реплика изменилась. Сбросьте черновик, чтобы загрузить актуальную версию.</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <button type="button" class="btn ghost sm" disabled={!changed || conflict} onclick={() => void save()}>Сохранить реплику дорожки</button>
  <button type="button" class="btn ghost sm" disabled={!changed && !conflict} onclick={() => adopt(captionCueDraft(cue))}>Сбросить черновик реплики</button>
</fieldset>
