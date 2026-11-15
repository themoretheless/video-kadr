<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { captionCueDraft, sameCaptionCueDraft, type CaptionCueDraft } from '$lib/composition/captionTracks'
  import type { CaptionCommandTimeInput } from '$lib/api'
  import type { CaptionCue } from '$lib/composition/types'
  import { nestedComposition, nestedHistory, updateNestedCaptionCue, type NestedCaptionTiming } from '$lib/state/nestedComposition.svelte'

  let { compositionId, trackId, cue, disabled = false }: {
    compositionId: string; trackId: string; cue: CaptionCue; disabled?: boolean
  } = $props()

  let draft = $state<CaptionCueDraft>({ text: '', speaker: '', cueId: '', settings: '', start: '', end: '' })
  let base = $state<CaptionCueDraft>({ text: '', speaker: '', cueId: '', settings: '', start: '', end: '' })
  let unit = $state<'seconds' | 'frames' | 'timecode'>('seconds')
  let startInput = $state(''), endInput = $state(''), dropFrame = $state(false)
  let error = $state('')
  let disposed = false
  const changed = $derived(unit === 'seconds'
    ? !sameCaptionCueDraft(draft, base)
    : !sameCaptionCueDraft({ ...draft, start: base.start, end: base.end }, base) || !!(startInput.trim() || endInput.trim()))
  const conflict = $derived(!sameCaptionCueDraft(base, captionCueDraft(cue)))

  $effect(() => () => { disposed = true })
  $effect(() => {
    const next = captionCueDraft(cue)
    untrack(() => { if (!changed) adopt(next) })
  })

  function adopt(next: CaptionCueDraft) {
    base = next
    draft = { ...next }
    startInput = ''; endInput = ''
    error = ''
  }

  function authoredInput(input: string): CaptionCommandTimeInput | undefined {
    const value = input.trim()
    if (!value) return
    if (unit === 'timecode') return { unit, value }
    const frame = Number(value)
    if (!/^[+-]?\d+$/.test(value) || !Number.isSafeInteger(frame)) throw new Error('Введите целое число кадров')
    return { unit: 'frames', value: frame }
  }

  async function save() {
    const composition = compositionId, track = trackId, id = cue.id
    let timing: NestedCaptionTiming | undefined
    if (unit !== 'seconds') {
      try {
        const start = authoredInput(startInput), end = authoredInput(endInput)
        if (start || end) timing = {
          ...(start ? { startInput: start } : {}), ...(end ? { endInput: end } : {}),
          ...(unit === 'timecode' ? { context: { playheadTick: nestedComposition.timeUs, dropFrame } } : {}),
        }
      } catch (reason) {
        error = reason instanceof Error ? reason.message : String(reason)
        return
      }
    }
    const next = unit === 'seconds' ? { ...draft } : { ...draft, start: base.start, end: base.end }
    const result = await updateNestedCaptionCue(composition, track, id, next, { ...base }, () => {
      if (disposed || compositionId !== composition || trackId !== track || cue.id !== id || disabled) {
        throw new Error('Выбранная реплика изменилась')
      }
    }, timing)
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
  {#if unit === 'seconds'}
    <label>Начало, с<input type="text" inputmode="decimal" bind:value={draft.start} /></label>
    <label>Конец, с<input type="text" inputmode="decimal" bind:value={draft.end} /></label>
  {:else}
    <label>Начало<input type="text" inputmode={unit === 'frames' ? 'numeric' : 'text'} placeholder="Без изменений" bind:value={startInput} /></label>
    <label>Конец<input type="text" inputmode={unit === 'frames' ? 'numeric' : 'text'} placeholder="Без изменений" bind:value={endInput} /></label>
    {#if unit === 'timecode'}
      <label><input type="checkbox" bind:checked={dropFrame} />Drop-frame</label>
    {/if}
  {/if}
  <label>Единицы времени реплики
    <select value={unit} onchange={event => {
      const value = event.currentTarget.value
      unit = value === 'frames' || value === 'timecode' ? value : 'seconds'
      startInput = ''; endInput = ''; error = ''
    }}>
      <option value="seconds">Секунды</option>
      <option value="frames">Кадры</option>
      <option value="timecode">Таймкод</option>
    </select>
  </label>
  {#if conflict}<p role="alert">Реплика изменилась. Сбросьте черновик, чтобы загрузить актуальную версию.</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <button type="button" class="btn ghost sm" disabled={!changed || conflict} onclick={() => void save()}>Сохранить реплику дорожки</button>
  <button type="button" class="btn ghost sm" disabled={!changed && !conflict} onclick={() => adopt(captionCueDraft(cue))}>Сбросить черновик реплики</button>
</fieldset>
