<script lang="ts">
  import { editFilmcraftAttributes, listFilmcraftEffects, type FilmcraftAttributesCommand, type FilmcraftEffectOption, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, selection, linked = true, clipboardText, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; selection: string[]; linked?: boolean; clipboardText?: string; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let motion = $state(true), opacity = $state(true), timeRemapping = $state(true), volume = $state(true), channelVolume = $state(true), panner = $state(true)
  let scaleTimes = $state(true)
  let effectsMode = $state<'all' | 'none' | 'selected'>('all')
  let effectIds = $state<string[]>([])
  let catalog = $state<FilmcraftEffectOption[]>([])
  let busy = $state(false), error = $state(''), status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function context() { return JSON.stringify([document, sequenceId, clipId, token, selection, linked, clipboardText, disabled, motion, opacity, timeRemapping, volume, channelVolume, panner, scaleTimes, effectsMode, effectIds]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => { void document; void sequenceId; void clipId; void token; void selection; void linked; void clipboardText; void disabled; return () => controller?.abort() })
  async function loadEffects() {
    if (!token || busy || disabled) return
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''
    try {
      const kinds = await Promise.all([listFilmcraftEffects(token, 'Video', current.signal), listFilmcraftEffects(token, 'Audio', current.signal)])
      if (!alive || current.signal.aborted || captured !== context()) return
      const intrinsic = new Set(['motion', 'opacity', 'time_remap', 'volume', 'channel_volume', 'panner', 'graphic_text', 'graphic_shape'])
      catalog = kinds.flat().filter(effect => !intrinsic.has(effect.id))
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
  async function run(command: FilmcraftAttributesCommand) {
    if (!token || busy || disabled || !sequenceId || !selection.length || command === 'edit.pasteAttributes' && !clipboardText) return
    const expected = { document, sequenceId, clipId, token, timelineSelection: [...selection] }
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''; status = ''
    try {
      const result = await editFilmcraftAttributes(token, command, { document, sequence: sequenceId, clips: [...selection], ...(command === 'edit.pasteAttributes' ? { clipboardText } : {}), motion, opacity, timeRemapping, volume, channelVolume, panner, scaleTimes, effects: effectsMode === 'all' ? true : effectsMode === 'none' ? false : [...effectIds], context: { activeSequence: sequenceId, selection: [...selection], linkedSelection: linked } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : `Изменено клипов: ${result.changedClipCount}`
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Атрибуты клипов</summary>
  <fieldset disabled={disabled || busy || !token}>
    <legend>Перенос и сброс атрибутов</legend>
    <label><input type="checkbox" bind:checked={motion} /> Движение</label>
    <label><input type="checkbox" bind:checked={opacity} /> Непрозрачность</label>
    <label><input type="checkbox" bind:checked={timeRemapping} /> Переназначение времени</label>
    <label><input type="checkbox" bind:checked={volume} /> Громкость</label>
    <label><input type="checkbox" bind:checked={channelVolume} /> Громкость каналов</label>
    <label><input type="checkbox" bind:checked={panner} /> Панорама</label>
    <label>Дополнительные эффекты <select aria-label="Атрибуты: эффекты Filmcraft" bind:value={effectsMode}><option value="all">Все</option><option value="none">Не переносить и не сбрасывать</option><option value="selected">Выбранные</option></select></label>
    {#if effectsMode === 'selected'}
      <button type="button" onclick={loadEffects}>Загрузить список эффектов</button>
      <select multiple aria-label="Атрибуты: выбранные эффекты Filmcraft" bind:value={effectIds}>{#each catalog as effect (effect.id)}<option value={effect.id}>{effect.name}</option>{/each}</select>
    {/if}
    <label><input type="checkbox" bind:checked={scaleTimes} /> Масштабировать время ключевых кадров при вставке</label>
    <button type="button" disabled={!selection.length || !clipboardText} onclick={() => run('edit.pasteAttributes')}>Вставить атрибуты</button>
    <button type="button" disabled={!selection.length} onclick={() => run('edit.removeAttributes')}>Удалить атрибуты</button>
  </fieldset>
  {#if busy}<span role="status">Обновляются атрибуты…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
