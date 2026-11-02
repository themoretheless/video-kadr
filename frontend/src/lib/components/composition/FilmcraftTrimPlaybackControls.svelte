<script lang="ts">
  import { FilmcraftTrimSessionClient } from '$lib/filmcraft/trim-session-client'
  import { ApiError, type FilmcraftImportResult, type FilmcraftTrimSessionResult, type FilmcraftTrimSessionAction, createFilmcraftTrimSession } from '$lib/api'
  let { token, input, project, disabled = false, onPreview, onCommit, onActive, onError }: {
    token: string | null; input: Parameters<typeof createFilmcraftTrimSession>[1]; project: FilmcraftImportResult; disabled?: boolean
    onPreview: (state: FilmcraftTrimSessionResult) => void
    onCommit: (before: FilmcraftImportResult, state: FilmcraftTrimSessionResult) => void
    onActive: (active: boolean) => void; onError: (message: string) => void
  } = $props()
  let client = $state<FilmcraftTrimSessionClient | null>(null)
  let playbackState = $state<FilmcraftTrimSessionResult | null>(null)
  let pending = $state(false)
  let recovery = $state(false)
  let baseline: FilmcraftImportResult | null = null
  let baselineDocument: string | null = null
  let creation: { token: string; input: Parameters<typeof createFilmcraftTrimSession>[1]; before: FilmcraftImportResult } | null = null
  let slow = $state(false)
  let looping = $state(true)
  let centerPlayhead = $state(false)
  let preroll = $state(0.5)
  let postroll = $state(1)
  let disposed = false
  const validRoll = $derived(Number.isFinite(preroll) && Number.isFinite(postroll) && preroll >= 0 && postroll >= 0)
  const clock = () => performance.now() / 1000
  function adopt(updated: FilmcraftTrimSessionResult, action?: FilmcraftTrimSessionAction) {
    const previous = playbackState
    playbackState = updated
    if (baseline && previous?.status.dynamic && !updated.status.dynamic) {
      if (action?.op !== 'cancel' && (updated.commit?.committed || updated.documentText !== baselineDocument)) onCommit(baseline, updated)
      baseline = null; baselineDocument = null
    }
    onPreview(updated)
  }
  async function ensureClient() {
    if (client) return client
    if (!token) throw new Error('Для монтажной сессии требуется вход')
    creation ??= { token, input: JSON.parse(JSON.stringify({ ...input, creationKey: crypto.randomUUID() })), before: project }
    const attempt = creation
    let created: FilmcraftTrimSessionClient
    try { created = await FilmcraftTrimSessionClient.create(attempt.token, attempt.input) }
    catch (error) {
      if (error instanceof ApiError && error.status >= 400 && error.status < 500) creation = null
      throw error
    }
    creation = null
    const before = attempt.before
    if (disposed) { await created.close(); throw new Error('Панель закрыта') }
    client = created; playbackState = created.state; baseline = before; baselineDocument = created.state.documentText; onActive(true)
    return created
  }
  async function run(action: FilmcraftTrimSessionAction) {
    if (pending || recovery || (disabled && !client)) return
    pending = true
    if (!client) onActive(true)
    try {
      const session = await ensureClient()
      if (action.op === 'shuttle' && !playbackState?.status.dynamic && !baseline) { baseline = project; baselineDocument = playbackState?.documentText || project.documentText }
      const updated = await session.command(action)
      if (!disposed) adopt(updated, action)
    } catch (error) {
      if (!disposed) { recovery = client?.needsRecovery || !!creation; if (!client && !creation) onActive(false); onError(error instanceof Error ? error.message : String(error)) }
    } finally { pending = false }
  }
  async function recover() {
    if ((!client && !creation) || pending) return
    pending = true
    try {
      if (!client) {
        const restored = await ensureClient()
        if (!disposed) { recovery = false; onPreview(restored.state) }
        return
      }
      const restored = await client.recover()
      if (disposed) return
      recovery = false
      if (restored) adopt(restored)
      else { client = null; playbackState = null; baseline = null; onActive(false); onError('Сессия истекла. Последнее подтверждённое состояние сохранено в панели.') }
    } catch (error) { if (!client && !creation) { recovery = false; onActive(false) }; onError(error instanceof Error ? error.message : String(error)) }
    finally { pending = false }
  }
  async function close() {
    if (!client || pending || recovery) return
    pending = true
    try {
      if (playbackState?.status.playing) adopt(await client.command({ op: 'stop', clock: clock() }), { op: 'stop' })
      await client.close(); client = null; playbackState = null; baseline = null; onActive(false)
    } catch (error) { recovery = client?.needsRecovery || false; onError(error instanceof Error ? error.message : String(error)) }
    finally { pending = false }
  }
  function keydown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.ctrlKey || event.metaKey || event.altKey) return
    const key = event.key.toLowerCase()
    if (!['j', 'k', 'l', 'escape', ' '].includes(key)) return
    event.preventDefault()
    if (key === 'j' || key === 'l') { if (input.points.length) void run({ op: 'shuttle', clock: clock(), direction: key === 'j' ? 'reverse' : 'forward', slow: slow || event.shiftKey }) }
    else if (client && key === 'k') void run({ op: 'stop', clock: clock() })
    else if (client && key === 'escape') void run({ op: 'cancel' })
    else if (key === ' ' && validRoll) void run({ op: 'around', clock: clock(), loop: looping, toggle: true, centerPlayhead, prerollSeconds: preroll, postrollSeconds: postroll })
  }
  $effect(() => {
    if (!playbackState?.status.playing || recovery) return
    const timer = setInterval(() => { if (!pending) void run({ op: 'tick', clock: clock() }) }, 100)
    return () => clearInterval(timer)
  })
  $effect(() => () => {
    disposed = true
    const closing = client
    closing?.cancelRequest()
    if (!closing && creation) { const attempt = creation; void FilmcraftTrimSessionClient.create(attempt.token, attempt.input).then(session => session.close()).catch(() => undefined) }
    if (closing) void closing.recover().then(() => closing.close()).catch(() => undefined)
  })
</script>

<div class="trim-playback" role="toolbar" aria-label="Динамическая подрезка Filmcraft" tabindex="0" onkeydown={keydown}>
  <button type="button" disabled={pending || recovery || (!client && (disabled || !token)) || !input.points.length} onclick={() => run({ op: 'shuttle', clock: clock(), direction: 'reverse', slow })}>Назад (J)</button>
  <button type="button" disabled={pending || recovery || !client} onclick={() => run({ op: 'stop', clock: clock() })}>Стоп (K)</button>
  <button type="button" disabled={pending || recovery || (!client && (disabled || !token)) || !input.points.length} onclick={() => run({ op: 'shuttle', clock: clock(), direction: 'forward', slow })}>Вперёд (L)</button>
  <label><input type="checkbox" bind:checked={slow} disabled={pending} /> Медленно</label>
  <button type="button" disabled={pending || recovery || !client} onclick={() => run({ op: 'cancel' })}>Отменить динамическую подрезку</button>
  <label>До, с <input aria-label="Предварительный просмотр, секунд" type="number" min="0" step="0.1" bind:value={preroll} disabled={pending} /></label>
  <label>После, с <input aria-label="Последующий просмотр, секунд" type="number" min="0" step="0.1" bind:value={postroll} disabled={pending} /></label>
  <label><input type="checkbox" bind:checked={looping} disabled={pending} /> Цикл</label>
  <label><input type="checkbox" bind:checked={centerPlayhead} disabled={pending} /> От курсора</label>
  <button type="button" disabled={pending || recovery || !validRoll || (!client && (disabled || !token))} onclick={() => run({ op: 'around', clock: clock(), loop: looping, toggle: true, centerPlayhead, prerollSeconds: preroll, postrollSeconds: postroll })}>Воспроизвести вокруг правки</button>
  {#if playbackState}<span role="status">{playbackState.status.dynamic ? `Сдвиг: ${playbackState.status.dynamic.offsetFrames} кадров · ${playbackState.status.dynamic.speed}×${playbackState.status.dynamic.atLimit ? ' · предел' : ''}` : playbackState.status.playing ? 'Просмотр вокруг правки' : 'Остановлено'}</span>{/if}
  {#if recovery}<button type="button" disabled={pending} onclick={recover}>Восстановить состояние сессии</button>{/if}
  {#if client}<button type="button" disabled={pending || recovery} onclick={close}>Закрыть монтажную сессию</button>{/if}
  <small>J/K/L, Esc и пробел работают, когда эта группа в фокусе. Кадры генераторов видны в мониторе стыка. Воспроизведение видео со звуком ещё не подключено.</small>
</div>
<style>
  .trim-playback { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; min-width: 0; }
  label { display: inline-flex; align-items: center; gap: 4px; }
  input[type="number"] { width: 68px; }
  small { flex-basis: 100%; }
</style>
