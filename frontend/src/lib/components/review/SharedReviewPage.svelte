<script lang="ts">
  import { ApiError, getSharedReview, type SharedReviewDto } from '$lib/api.js'
  import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'

  interface Props { token: string }
  let { token }: Props = $props()
  let review = $state<SharedReviewDto | null>(null)
  let loading = $state(true)
  let error = $state('')
  let loadRevision = 0

  $effect(() => { void load(token, ++loadRevision) })

  async function load(requestToken = token, revision = ++loadRevision): Promise<void> {
    loading = true
    error = ''
    try {
      const result = await getSharedReview(requestToken)
      if (revision !== loadRevision) return
      review = result
    } catch (cause) {
      if (revision !== loadRevision) return
      review = null
      error = cause instanceof ApiError && cause.status === 404
        ? 'Ссылка недействительна, истекла или была отозвана.'
        : 'Не удалось загрузить ревью. Попробуйте ещё раз.'
    } finally {
      if (revision === loadRevision) loading = false
    }
  }

  function formatTick(tick: number): string {
    const totalSeconds = Math.max(0, tick) / COMPOSITION_TIME_BASE
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = totalSeconds - minutes * 60
    return `${minutes}:${seconds.toFixed(3).padStart(6, '0')}`
  }

  function formatExpiry(value: number): string {
    return new Intl.DateTimeFormat('ru', { dateStyle: 'medium', timeStyle: 'short' })
      .format(new Date(value * 1000))
  }
</script>

<svelte:head><title>{review ? `${review.projectName} — Review` : 'Review'}</title></svelte:head>

<main class="shared-review-shell">
  <header class="shared-review-brand"><span aria-hidden="true">🎬</span><strong>Видеоредактор</strong><span>Review</span></header>
  {#if loading}
    <section class="shared-review-state" aria-live="polite"><p>Загружаю ревью…</p></section>
  {:else if error}
    <section class="shared-review-state error-state" role="alert">
      <h1>Ревью недоступно</h1><p>{error}</p>
      <button class="btn primary" type="button" onclick={() => void load(token)}>Повторить</button>
    </section>
  {:else if review}
    <section class="shared-review-hero">
      <div><p class="eyebrow">Только просмотр</p><h1>{review.projectName}</h1></div>
      <p>Доступ до <time datetime={new Date(review.expiresAt * 1000).toISOString()}>{formatExpiry(review.expiresAt)}</time></p>
    </section>
    <section class="shared-review-content" aria-label="Комментарии ревью">
      <div class="shared-review-summary">
        <strong>{review.threads.length}</strong>
        <span>{review.threads.length === 1 ? 'ветка комментариев' : 'веток комментариев'}</span>
      </div>
      {#if review.threads.length === 0}
        <p class="shared-review-empty">Комментариев пока нет.</p>
      {:else}
        <ol class="shared-review-list">
          {#each review.threads as thread (thread.id)}
            <li class:resolved={thread.resolvedAt != null}>
              <div class="shared-review-thread-head">
                <time>{formatTick(thread.comments[0]?.timelineTick ?? 0)}</time>
                <span>{thread.resolvedAt == null ? 'Открыто' : 'Закрыто'}</span>
              </div>
              <ol>
                {#each thread.comments as comment (comment.id)}
                  <li><strong>{comment.author}</strong><p>{comment.body}</p></li>
                {/each}
              </ol>
            </li>
          {/each}
        </ol>
      {/if}
    </section>
    <footer class="shared-review-foot">Ссылка предоставляет только чтение комментариев и не раскрывает файлы проекта.</footer>
  {/if}
</main>

<style>
  :global(body) { min-width: 320px; background: #0b0d12; }
  .shared-review-shell { width: min(860px, calc(100% - 32px)); margin: 0 auto; padding: 24px 0 48px; color: var(--text); }
  .shared-review-brand, .shared-review-hero, .shared-review-thread-head { display: flex; align-items: center; }
  .shared-review-brand { gap: 8px; color: var(--muted); font-size: .82rem; }
  .shared-review-brand strong { color: var(--text); }
  .shared-review-brand span:last-child { padding-left: 8px; border-left: 1px solid var(--border); }
  .shared-review-hero { justify-content: space-between; gap: 24px; margin: 42px 0 18px; }
  .shared-review-hero h1, .shared-review-state h1 { margin: 3px 0 0; font-size: clamp(1.65rem, 5vw, 2.5rem); }
  .shared-review-hero > p { color: var(--muted); font-size: .78rem; text-align: right; }
  .eyebrow { margin: 0; color: var(--accent); font-size: .72rem; font-weight: 750; letter-spacing: .09em; text-transform: uppercase; }
  .shared-review-content, .shared-review-state { border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--panel); box-shadow: var(--shadow); }
  .shared-review-content { padding: 18px; }
  .shared-review-state { margin-top: 42px; padding: 48px 24px; text-align: center; }
  .shared-review-state p { color: var(--muted); }
  .shared-review-summary { display: flex; align-items: baseline; gap: 7px; padding-bottom: 14px; border-bottom: 1px solid var(--border); }
  .shared-review-summary strong { font-size: 1.35rem; }
  .shared-review-summary span, .shared-review-empty { color: var(--muted); font-size: .82rem; }
  .shared-review-list, .shared-review-list ol { margin: 0; padding: 0; list-style: none; }
  .shared-review-list > li { padding: 16px 0; border-bottom: 1px solid var(--border); }
  .shared-review-list > li:last-child { padding-bottom: 0; border: 0; }
  .shared-review-list > li.resolved { opacity: .65; }
  .shared-review-thread-head { justify-content: space-between; margin-bottom: 12px; }
  .shared-review-thread-head time { color: var(--accent); font: 700 .8rem ui-monospace, monospace; }
  .shared-review-thread-head span { color: var(--muted); font-size: .72rem; }
  .shared-review-list ol { display: grid; gap: 10px; }
  .shared-review-list ol li { padding-left: 12px; border-left: 2px solid var(--border-strong); }
  .shared-review-list ol strong { font-size: .75rem; }
  .shared-review-list ol p { margin: 3px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .shared-review-foot { padding-top: 14px; color: var(--muted); font-size: .7rem; text-align: center; }
  @media (max-width: 560px) { .shared-review-hero { align-items: flex-start; flex-direction: column; } .shared-review-hero > p { text-align: left; } }
</style>
