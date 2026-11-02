<script lang="ts">
  import { replaceFilmcraftClips, type FilmcraftReplaceCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, selection, projectSelection, sourceItem, sourcePlayhead, timeFrames, timeTicks, items, linked = true, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; selection: string[]; projectSelection: string[]; sourceItem: string; sourcePlayhead: string; timeFrames: string; timeTicks?: string; items: NonNullable<FilmcraftImportResult['projectItems']>; linked?: boolean; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let item = $state(''), keepSourceIn = $state(false)
  let busy = $state(false), error = $state(''), status = $state('')
  let warnings = $state<string[]>([])
  let controller: AbortController | undefined
  let alive = true
  const eligible = $derived(items.filter(item => item.kind !== 'Graphic'))
  function context() { return JSON.stringify([document, sequenceId, clipId, token, selection, projectSelection, sourceItem, sourcePlayhead, timeFrames, timeTicks, linked, disabled, item, keepSourceIn]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => { void document; void sequenceId; void clipId; void token; void selection; void projectSelection; void sourceItem; void sourcePlayhead; void timeFrames; void timeTicks; void linked; void disabled; return () => controller?.abort() })
  async function run(command: FilmcraftReplaceCommand) {
    if (!token || busy || disabled || !sequenceId || !selection.length || (command === 'clip.replaceFromBin' ? !item && !projectSelection.some(id => eligible.some(i => i.id === id)) : !sourceItem)) return
    const expected = { document, sequenceId, clipId, token, timelineSelection: [...selection], projectSelection: [...projectSelection], sourceItem: sourceItem || null, sourcePlayhead, timeFrames, timeTicks: timeTicks || null }
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''; status = ''; warnings = []
    try {
      const result = await replaceFilmcraftClips(token, command, { document, sequence: sequenceId, clips: [...selection], ...(command === 'clip.replaceFromBin' ? { ...(item ? { item } : {}), keepSourceIn } : {}), context: { activeSequence: sequenceId, selection: [...selection], projectSelection: [...projectSelection], sourceItem: sourceItem || null, sourcePlayhead, ...(timeTicks ? { playhead: timeTicks } : { playheadFrames: timeFrames }), linkedSelection: linked } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      if (onedit(result, expected) === false) status = 'Результат не применён: контекст изменился'
      else {
        status = `Заменено клипов: ${result.replacedClipIds.length}`
        warnings = [...result.shortClips.map(short => `Клип ${short.clip}: не хватает ${short.shortByFrames} кадров медиа`), ...(result.sourceInClamped.length ? [`Source In сдвинут в диапазон подклипа: ${result.sourceInClamped.length}`] : [])]
      }
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Заменить клипы</summary>
  <fieldset disabled={disabled || busy || !token || !selection.length}>
    <legend>Источник замены</legend>
    <button type="button" disabled={!sourceItem} onclick={() => run('clip.replaceFromSource')}>Заменить из Source Monitor</button>
    <button type="button" disabled={!sourceItem} onclick={() => run('clip.replaceFromSourceMatchFrame')}>Заменить с совмещением кадра</button>
    <label>Элемент проекта <select aria-label="Исходник замены Filmcraft" value={item} onchange={(event) => { item = event.currentTarget.value }}><option value="">Из выделения проекта</option>{#each eligible as entry (entry.id)}<option value={entry.id}>{entry.name}</option>{/each}</select></label>
    <label><input type="checkbox" bind:checked={keepSourceIn} /> Сохранить Source In каждого клипа</label>
    <button type="button" disabled={!item && !projectSelection.some(id => eligible.some(i => i.id === id))} onclick={() => run('clip.replaceFromBin')}>Заменить из проекта</button>
  </fieldset>
  {#if busy}<span role="status">Заменяются клипы…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#each warnings as warning, index (index)}<p role="status">{warning}</p>{/each}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
