<script lang="ts">
  import { beginYouTubeConnection, cancelJob, disconnectYouTube, getYouTubeConnectionStatus, pollJob, publishYouTube, type YouTubeConnectionStatusDto } from '$lib/api.js'
  import { eventListener, listenMany } from '../../../composables/globalListeners.js'
  import { authState } from '$lib/state/auth.svelte.js'
  import { compositionState } from '$lib/state/composition.svelte.js'

  let status = $state<YouTubeConnectionStatusDto | null>(null)
  let busy = $state(false)
  let message = $state('')
  let error = $state('')
  let authorizationUrl = $state('')
  let loadedToken = $state<string | null>(null)
  let title = $state('')
  let description = $state('')
  let privacyStatus = $state<'private' | 'unlisted' | 'public'>('private')
  let publishJobId = $state<string | null>(null)
  let publishProgress = $state<number | null>(null)
  let publishedUrl = $state('')

  $effect(() => {
    const token = authState.token
    if (!token) { status = null; loadedToken = null; return }
    if (loadedToken !== token) { loadedToken = token; void refresh(token) }
  })

  $effect(() => listenMany([[window, 'focus', eventListener(() => {
    if (authState.token && authorizationUrl) void refresh(authState.token)
  })]]))

  async function refresh(token = authState.token): Promise<void> {
    if (!token) return
    try {
      status = await getYouTubeConnectionStatus(token)
      if (status.connected) { authorizationUrl = ''; message = 'YouTube подключён — канал готов к публикации.' }
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }

  async function connect(): Promise<void> {
    const token = authState.token
    if (!token || busy) return
    busy = true; error = ''; message = ''
    const popup = window.open('', 'video-kadr-youtube-oauth', 'popup,width=640,height=760')
    try {
      const response = await beginYouTubeConnection(token)
      authorizationUrl = response.authorizationUrl
      if (popup) popup.location.href = response.authorizationUrl
      else message = 'Браузер заблокировал окно. Откройте ссылку подключения ниже.'
    } catch (cause) {
      popup?.close()
      error = cause instanceof Error ? cause.message : String(cause)
    } finally { busy = false }
  }

  async function disconnect(): Promise<void> {
    const token = authState.token
    if (!token || busy) return
    busy = true; error = ''; message = ''
    try {
      await disconnectYouTube(token)
      status = status ? { ...status, connected: false } : null
      authorizationUrl = ''
      message = 'Локальное подключение YouTube удалено.'
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function publish(): Promise<void> {
    const token = authState.token
    const output = compositionState.export.result
    if (!token || !output || busy) return
    busy = true; error = ''; message = ''; publishedUrl = ''; publishProgress = 0
    try {
      const started = await publishYouTube(token, {
        outputId: output.id, title: title.trim() || compositionState.projectName,
        description, privacyStatus,
      })
      publishJobId = started.jobId
      const job = await pollJob(started.jobId, (next) => { publishProgress = next.progress ?? publishProgress })
      const result = job.result as { url?: unknown } | undefined
      publishedUrl = typeof result?.url === 'string' ? result.url : ''
      message = 'Видео опубликовано на YouTube.'
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false; publishJobId = null }
  }

  async function cancelPublish(): Promise<void> {
    if (publishJobId) await cancelJob(publishJobId)
  }
</script>

{#if authState.token}
  <section class="youtube-publish card" aria-label="Публикация на YouTube">
    <div>
      <strong>YouTube</strong>
      <p>{status?.connected ? 'Канал подключён' : status?.configured ? 'Подключите канал для прямой публикации' : 'OAuth не настроен на сервере'}</p>
    </div>
    {#if status?.connected}
      <button class="btn ghost sm danger" disabled={busy} onclick={() => void disconnect()}>Отключить</button>
    {:else}
      <button class="btn primary sm" disabled={busy || !status?.configured} onclick={() => void connect()}>{busy ? 'Подключаю…' : 'Подключить YouTube'}</button>
    {/if}
    {#if authorizationUrl}<a class="btn ghost sm" href={authorizationUrl} target="video-kadr-youtube-oauth" rel="noopener noreferrer">Открыть авторизацию</a>{/if}
    {#if status?.connected && compositionState.export.result}
      <div class="youtube-fields">
        <input aria-label="Название видео YouTube" maxlength="100" placeholder={compositionState.projectName} bind:value={title} />
        <textarea aria-label="Описание видео YouTube" maxlength="5000" placeholder="Описание" bind:value={description}></textarea>
        <select aria-label="Видимость видео YouTube" bind:value={privacyStatus}>
          <option value="private">Приватное</option><option value="unlisted">По ссылке</option><option value="public">Публичное</option>
        </select>
        <button class="btn primary sm" disabled={busy} onclick={() => void publish()}>Опубликовать последний экспорт</button>
        {#if publishJobId}<button class="btn ghost sm danger" onclick={() => void cancelPublish()}>Отменить</button>{/if}
      </div>
    {:else if status?.connected}
      <p class="youtube-publish-message">Сначала экспортируйте композицию.</p>
    {/if}
    {#if publishProgress !== null && busy}<p class="youtube-publish-message" role="status">Загрузка: {Math.round(publishProgress)}%</p>{/if}
    {#if publishedUrl}<a href={publishedUrl} target="_blank" rel="noopener noreferrer">Открыть видео на YouTube</a>{/if}
    {#if message}<p class="youtube-publish-message" role="status">{message}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
{/if}

<style>
  .youtube-publish { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
  .youtube-publish div { flex: 1 1 16rem; }
  .youtube-publish p { margin: .2rem 0 0; color: var(--text-muted); }
  .youtube-publish-message { flex-basis: 100%; color: var(--text-muted); }
  .youtube-fields { display: grid; grid-template-columns: minmax(10rem, 1fr) minmax(10rem, 2fr) auto auto; gap: .5rem; flex-basis: 100%; }
  .youtube-fields textarea { min-height: 2.25rem; resize: vertical; }
  @media (max-width: 800px) { .youtube-fields { grid-template-columns: 1fr; } }
</style>
