<script lang="ts">
  import { getSceneModelStatus, installSceneModel, type SceneModelStatus } from '../api.js'

  let { token }: { token: string } = $props()
  let status = $state<SceneModelStatus | null>()
  let pending = $state(false)
  let error = $state('')
  let timer: ReturnType<typeof setTimeout> | undefined
  const lifecycle = new AbortController()
  const labels: Record<SceneModelStatus['phase'], string> = {
    missing: 'Модель ещё не установлена',
    installing: 'Модель загружается…',
    failed: 'Загрузка модели не завершилась',
    ready: 'Модель готова к работе',
    offline: 'Локальная служба Ollama недоступна',
  }

  $effect(() => {
    queueMicrotask(() => void update())
    return () => {
      clearTimeout(timer)
      lifecycle.abort()
    }
  })

  async function update(install = false) {
    if (pending || lifecycle.signal.aborted) return
    clearTimeout(timer)
    pending = true
    error = ''
    try {
      const result = install
        ? await installSceneModel(token, lifecycle.signal)
        : await getSceneModelStatus(token, lifecycle.signal)
      if (!lifecycle.signal.aborted) status = result
    } catch (cause) {
      if (!lifecycle.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (!lifecycle.signal.aborted) {
        pending = false
        if (status?.phase === 'installing') timer = setTimeout(() => void update(), 3000)
      }
    }
  }
</script>

{#if status}
  <p><strong>{status.model}</strong> <span role="status">{labels[status.phase]}</span></p>
  {#if status.canInstall && (status.phase === 'missing' || status.phase === 'failed')}
    <button class="btn ghost sm" type="button" disabled={pending} onclick={() => void update(true)}>
      {status.phase === 'failed' ? 'Повторить загрузку' : 'Установить модель'}
    </button>
  {/if}
{:else if status === null}
  <p>Установка локальной модели в этом приложении не настроена.</p>
{:else if pending}
  <p role="status">Проверяю локальную модель…</p>
{/if}
{#if error}<p role="alert">{error}</p>{/if}
{#if (status && status.phase !== 'installing') || error}
  <button class="btn ghost sm" type="button" disabled={pending} onclick={() => void update()}>Обновить состояние</button>
{/if}
