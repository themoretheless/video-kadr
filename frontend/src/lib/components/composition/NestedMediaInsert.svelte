<script lang="ts">
  import { untrack } from 'svelte'
  import { nestedComposition, nestedHistory, insertNestedMedia } from '$lib/state/nestedComposition.svelte'
  import { compositionSourceFromLibraryEntry, inferMediaType } from '$lib/state/composition.svelte'
  import { state as library } from '$lib/state/store.svelte'
  import { insertNativeMedia, insertNativeMediaAtFirstFree } from '$lib/composition/nativeClipInsert'
  let { disabled = false }: { disabled?: boolean } = $props()
  const project = $derived(nestedComposition.document)
  const active = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const tracks = $derived(project?.compositions[active]?.content.tracks.filter(track => ['video', 'audio', 'image'].includes(track.kind)) ?? [])
  let sourceId = $state(''), trackId = $state(''), start = $state(0), sourceIn = $state(0), duration = $state(5), videoAudio = $state(true), automatic = $state(false)
  const selectedKind = $derived.by(() => { const entry = library.library.find(entry => entry.id === sourceId); return entry ? inferMediaType(entry) : null })
  $effect(() => { if (active) { sourceId = ''; trackId = ''; start = untrack(() => nestedComposition.timeUs / 1000000) } })
  function plan() {
    if (!project) throw new Error('Нет проекта')
    const entry = library.library.find(entry => entry.id === sourceId && entry.kind === 'source')
    if (!entry) throw new Error('Выберите исходный файл из медиатеки')
    if (!entry.url.trim()) throw new Error('У файла нет доступного адреса')
    const source = compositionSourceFromLibraryEntry(entry)
    const reserved = new Set(project.compositions[active]?.content.tracks.flatMap(track => track.clips.map(clip => clip.id)) ?? [])
    const trackIds = new Set(project.compositions[active]?.content.tracks.map(track => track.id) ?? [])
    let serial = 0; while (reserved.has(`insert-preview-${serial}`) || reserved.has(`insert-preview-${serial}-audio`) || trackIds.has(`insert-track-preview-${serial}`)) serial++
    const request = { trackId, clipId: `insert-preview-${serial}`, startUs: Math.round(start * 1000000), sourceInUs: Math.round(sourceIn * 1000000), durationUs: Math.round(duration * 1000000), videoAudio, automatic, newTrackId: `insert-track-preview-${serial}` }
    if (automatic) insertNativeMediaAtFirstFree(project, active, source, request)
    else insertNativeMedia(project, active, source, request)
    return { source, request }
  }
  const reason = $derived.by(() => { try { plan(); return null } catch (error) { return error instanceof Error ? error.message : String(error) } })
  function chooseSource(id: string): void {
    sourceId = id
    const entry = library.library.find(entry => entry.id === id && entry.kind === 'source')
    if (!entry) return
    try { const source = compositionSourceFromLibraryEntry(entry); sourceIn = 0; duration = source.kind === 'image' ? 5 : source.durationTicks / 1000000 } catch { /* plan displays metadata admission errors */ }
  }
  function insert(): void { try { const { source, request } = plan(); void insertNestedMedia(active, source, { ...request, clipId: `clip-${crypto.randomUUID()}`, newTrackId: `track-${crypto.randomUUID()}` }) } catch (error) { nestedHistory.error = error instanceof Error ? error.message : String(error) } }
</script>
{#if project}
  <fieldset class="composition-fieldset" disabled={disabled || nestedHistory.busy}>
    <legend>Добавить медиа в таймлайн</legend>
    <label>Файл <select aria-label="Медиа для нативного таймлайна" value={sourceId} onchange={event => chooseSource(event.currentTarget.value)}><option value="">Выберите файл</option>{#each library.library.filter(entry => entry.kind === 'source') as entry (entry.id)}<option value={entry.id}>{entry.filename}</option>{/each}</select></label>
    <label><input type="checkbox" aria-label="Автоматический выбор дорожки" bind:checked={automatic} /> Выбрать свободную дорожку автоматически</label>
    <label>Дорожка <select disabled={automatic} aria-label="Дорожка для нового клипа" value={trackId} onchange={event => { trackId = event.currentTarget.value }}><option value="">Выберите дорожку</option>{#each tracks as track (track.id)}<option value={track.id} disabled={track.locked}>{track.name}</option>{/each}</select></label>
    {#if (automatic && selectedKind === 'video') || (!automatic && tracks.find(track => track.id === trackId)?.kind === 'video')}<label><input type="checkbox" aria-label="Использовать звук нового видео" bind:checked={videoAudio} /> Использовать звук видео</label>{/if}
    {#if automatic && selectedKind === 'video' && videoAudio}<p>Видео и его звук добавляются отдельными клипами в одно время.</p>{/if}
    <label>Начало в таймлайне, с <input aria-label="Начало нового клипа" type="number" min="0" step="0.001" bind:value={start} /></label>
    <label>Начало в файле, с <input aria-label="Начало нового клипа в файле" type="number" min="0" step="0.001" bind:value={sourceIn} /></label>
    <label>Длительность, с <input aria-label="Длительность нового клипа" type="number" min="0.001" step="0.001" bind:value={duration} /></label>
    <button class="btn ghost sm" type="button" disabled={reason !== null} title={reason ?? undefined} onclick={insert}>Добавить медиа</button>
  </fieldset>
{/if}
