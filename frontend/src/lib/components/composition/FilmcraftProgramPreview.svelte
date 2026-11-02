<script lang="ts">
  import { untrack } from 'svelte'
  import { getFilmcraftPreviewFrame, type FilmcraftPreviewInput, type FilmcraftPreviewFrame } from '$lib/api'
  import { MonitorQueue } from '$lib/filmcraft/monitor-queue'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'
  import FilmcraftAnalysisControls from './FilmcraftAnalysisControls.svelte'
  let { document: source, sequenceId, timeFrames, token, sources = {} }: { sources?: Record<string, string>; document: string; sequenceId: string; timeFrames: string; token: string | null } = $props()
  let enabled = $state(false)
  let effects = $state(true)
  let captions = $state(true)
  let retry = $state(0)
  let frame = $state<FilmcraftPreviewFrame | null>(null)
  let pending = $state(false)
  let error = $state('')
  type Request = { input: FilmcraftPreviewInput; token: string | null; retry: number }
  const queue = new MonitorQueue<Request, FilmcraftPreviewFrame>(
    (request, signal) => getFilmcraftPreviewFrame(request.input, signal, request.token),
    { reset: () => { frame = null; error = '' }, result: (value) => { frame = value; error = '' }, error: (failure) => { frame = null; error = failure instanceof Error ? failure.message : String(failure) }, pending: (value) => { pending = value } },
    (a, b) => a.retry === b.retry && a.token === b.token && JSON.stringify(a.input) === JSON.stringify(b.input)
  )
  $effect(() => {
    const active = enabled; const input = { document: source, sequenceId, timeFrames, effects, captions, sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)) }; const auth = token; const attempt = retry
    let valid = /^-?\d{1,19}$/.test(timeFrames)
    if (valid) { const n = BigInt(timeFrames); valid = n >= -9223372036854775808n && n <= 9223372036854775807n }
    if (!active || !valid || !sequenceId) { untrack(() => queue.clear()); return }
    const key = JSON.stringify([sequenceId, effects, captions, auth, input.sources])
    untrack(() => { error = ''; queue.submit(key, { input, token: auth, retry: attempt }) })
  })
  $effect(() => () => queue.clear())
</script>
<section aria-label="Предпросмотр композиции Filmcraft">
  <label><input type="checkbox" bind:checked={enabled} />Предпросмотр композиции</label>
  {#if enabled}
    <label><input type="checkbox" bind:checked={effects} />Эффекты</label>
    <label><input type="checkbox" bind:checked={captions} />Субтитры</label>
    <FilmcraftAnalysisControls input={{ document: source, sequenceId, timeFrames, effects, captions, sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)) }} {token} onready={() => retry++} />
    {#if pending}<small role="status">Подготовка кадра…</small>{/if}
    {#if frame}<div class="frame"><FilmcraftMonitorFrame {frame} label="Кадр композиции Filmcraft" /></div>{/if}
    {#if error}<small role="alert">{error}</small><button type="button" onclick={() => retry++}>Повторить предпросмотр</button>{/if}
  {/if}
</section>
<style>
  section { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; flex-basis: 100%; }
  label { display: inline-flex; align-items: center; gap: 0.25rem; }
  .frame, small { flex-basis: 100%; }
</style>
