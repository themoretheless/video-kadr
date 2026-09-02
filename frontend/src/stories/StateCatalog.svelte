<script lang="ts">
  interface Props {
    state: 'empty' | 'loading' | 'error' | 'ready'
    locale?: 'ru' | 'en'
    longContent?: boolean
    reducedMotion?: boolean
  }
  let {
    state,
    locale = 'ru',
    longContent = false,
    reducedMotion = false,
  }: Props = $props()
  const copy = $derived(locale === 'ru'
    ? { title: 'Медиатека', empty: 'Импортируйте видео, чтобы начать', loading: 'Загружаю медиатеку…', error: 'Не удалось загрузить медиатеку', retry: 'Повторить' }
    : { title: 'Media library', empty: 'Import a video to get started', loading: 'Loading media library…', error: 'Could not load media library', retry: 'Retry' })
  const items = $derived(longContent ? Array.from({ length: 18 }, (_, index) => `${locale === 'ru' ? 'Очень длинное имя интервью' : 'Very long interview filename'} ${index + 1}.mp4`) : ['intro.mp4', 'interview.mp4', 'b-roll.mp4'])
</script>

<main class:reduced-motion={reducedMotion} aria-labelledby="catalog-title">
  <section class="card" aria-busy={state === 'loading'}>
    <h1 id="catalog-title">{copy.title}</h1>
    {#if state === 'empty'}
      <div class="empty" role="status">{copy.empty}</div>
    {:else if state === 'loading'}
      <div class="loading" role="status"><span aria-hidden="true"></span>{copy.loading}</div>
    {:else if state === 'error'}
      <div class="error" role="alert"><p>{copy.error}</p><button>{copy.retry}</button></div>
    {:else}
      <ul aria-label={copy.title}>
        {#each items as item (item)}<li><span aria-hidden="true">▣</span><span>{item}</span></li>{/each}
      </ul>
    {/if}
  </section>
</main>

<style>
  main { box-sizing: border-box; min-height: 100vh; padding: 24px; color: var(--text); background: var(--bg); font: 14px/1.45 system-ui, sans-serif; }
  .card { max-width: 720px; margin: 0 auto; padding: 20px; background: var(--panel); border: 1px solid var(--border); border-radius: var(--radius); }
  h1 { margin: 0 0 16px; font-size: 20px; }
  .empty, .loading, .error { padding: 32px 16px; text-align: center; color: var(--muted); background: var(--panel-2); border-radius: var(--radius-sm); }
  .loading { display: flex; gap: 10px; justify-content: center; align-items: center; }
  .loading span { width: 14px; height: 14px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin .8s linear infinite; }
  .error { color: var(--danger); }
  button { padding: 7px 12px; color: var(--text); background: var(--panel-3); border: 1px solid var(--border-strong); border-radius: var(--radius-sm); }
  ul { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
  li { display: flex; gap: 8px; min-width: 0; padding: 9px 10px; background: var(--panel-2); border-radius: var(--radius-sm); }
  li span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .reduced-motion * { animation: none !important; scroll-behavior: auto !important; }
  @keyframes spin { to { transform: rotate(1turn); } }
  @media (max-width: 480px) { main { padding: 8px; } .card { padding: 12px; } }
</style>
