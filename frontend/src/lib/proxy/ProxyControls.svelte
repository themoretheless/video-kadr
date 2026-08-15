<script lang="ts">
  import type { Capabilities, MediaEntry } from '$lib/types.js'
  import {
    defaultProxyQuality,
    detectProxyPlatform,
    formatProxyProfile,
    proxyCodecOptions,
  } from './model.js'
  import {
    proxyController as defaultController,
    type ProxyController,
  } from './state.svelte.js'
  import type { ProxyCodec, ProxyProfile } from './types.js'

  interface Props {
    entry: MediaEntry
    capabilities: Capabilities | null
    controller?: ProxyController
  }

  let { entry, capabilities, controller = defaultController }: Props = $props()
  const platform = detectProxyPlatform()
  let maxWidth = $state(960)
  let codec = $state<ProxyCodec>('h264')
  let includeAudio = $state(true)
  let loadedSourceId = $state('')
  let codecOptions = $derived(proxyCodecOptions(capabilities, platform))
  let selectedCodec = $derived(codecOptions.find((option) => option.codec === codec))
  let proxyUi = $derived(controller.state.sources[entry.id])
  let preference = $derived(controller.state.preferences[entry.id])
  let selectedArtifact = $derived(
    preference?.mode === 'proxy'
      ? proxyUi?.list?.proxies.find((artifact) => artifact.key === preference.key)
      : undefined,
  )
  let stalePreference = $derived(
    preference?.mode === 'proxy' && proxyUi?.list !== null && proxyUi?.list !== undefined && !selectedArtifact,
  )
  let matchingJob = $derived(proxyUi?.list?.jobs.some((job) =>
    job.profile.maxWidth === maxWidth
      && job.profile.codec === codec
      && job.profile.includeAudio === includeAudio,
  ) ?? false)

  $effect(() => {
    if (selectedCodec?.available) return
    const first = codecOptions.find((option) => option.available)
    if (first) codec = first.codec
  })

  $effect(() => {
    if (loadedSourceId === entry.id) return
    loadedSourceId = entry.id
    void controller.refresh(entry.id)
  })

  function profile(): ProxyProfile {
    return {
      maxWidth,
      codec,
      quality: defaultProxyQuality(codec),
      includeAudio,
    }
  }

  function generate(): void {
    void controller.generate(entry.id, profile())
  }

  function remove(key: string): void {
    void controller.remove(entry.id, key)
  }

  function fmtSize(bytes: number): string {
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} ГБ`
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} МБ`
    return `${Math.max(1, Math.round(bytes / 1024))} КБ`
  }

  function stageLabel(stage?: string): string {
    if (stage === 'queued') return 'В очереди'
    if (stage === 'processing') return 'Кодирование'
    return stage || 'Подготовка'
  }

  function previewAvailable(codec: ProxyCodec): boolean {
    return codec !== 'prores_proxy' || platform === 'mac'
  }
</script>

<section class="proxy-controls" aria-label={`Proxy для «${entry.title?.trim() || entry.filename}»`}>
  <div class="proxy-heading">
    <div>
      <strong>Proxy для предпросмотра</strong>
      <p>Экспорт всегда использует оригинальный source.</p>
    </div>
    <button class="btn ghost sm" type="button" disabled={proxyUi?.phase === 'loading'} onclick={() => void controller.refresh(entry.id)}>
      Обновить
    </button>
  </div>

  <fieldset class="proxy-profile" disabled={proxyUi?.submitting}>
    <legend>Профиль</legend>
    <label>
      <span>Максимальная ширина</span>
      <select bind:value={maxWidth}>
        <option value={480}>480 px</option>
        <option value={720}>720 px</option>
        <option value={960}>960 px</option>
      </select>
    </label>
    <label>
      <span>Кодек</span>
      <select bind:value={codec}>
        {#each codecOptions as option (option.codec)}
          <option value={option.codec} disabled={!option.available}>{option.label}</option>
        {/each}
      </select>
    </label>
    <label class="proxy-audio">
      <input type="checkbox" bind:checked={includeAudio} />
      Включить звук
    </label>
    <button
      class="btn primary sm"
      type="button"
      disabled={!selectedCodec?.available || proxyUi?.submitting || matchingJob}
      title={selectedCodec?.available ? '' : selectedCodec?.reason}
      onclick={generate}
    >{matchingJob ? 'Создаётся…' : 'Создать proxy'}</button>
  </fieldset>

  {#if !codecOptions.some((option) => option.available)}
    <p class="proxy-note" role="status">Генерация недоступна: {codecOptions.map((option) => `${option.label}: ${option.reason}`).join('; ')}.</p>
  {:else if selectedCodec?.reason && !selectedCodec.available}
    <p class="proxy-note" role="status">{selectedCodec.reason}</p>
  {/if}

  {#if proxyUi?.error}
    <p class="proxy-error" role="alert">{proxyUi.error}</p>
  {/if}
  {#if proxyUi?.playbackError}
    <p class="proxy-error" role="alert">{proxyUi.playbackError}</p>
  {/if}
  {#if stalePreference}
    <p class="proxy-warning" role="status">Выбранный proxy устарел, удалён или ещё не готов. Предпросмотр использует оригинал.</p>
  {/if}

  <div class="proxy-original">
    <button
      class="btn ghost sm"
      class:active={preference?.mode !== 'proxy'}
      type="button"
      aria-pressed={preference?.mode !== 'proxy'}
      onclick={() => controller.setOriginal(entry.id)}
    >Использовать оригинал</button>
    {#if proxyUi?.phase === 'loading'}<span role="status">Проверяем proxy…</span>{/if}
  </div>

  {#if proxyUi?.list?.jobs.length}
    <div class="proxy-group">
      <h3>Создаются</h3>
      <ul>
        {#each proxyUi.list.jobs as job (job.jobId)}
          <li>
            <div class="proxy-item-main">
              <strong>{formatProxyProfile(job.profile)}</strong>
              <span>{stageLabel(job.stage)}{typeof job.progress === 'number' ? ` · ${Math.round(job.progress)}%` : ''}</span>
              {#if typeof job.progress === 'number'}
                <progress max="100" value={job.progress} aria-label={`Прогресс ${formatProxyProfile(job.profile)}`}>{job.progress}%</progress>
              {/if}
            </div>
            <button class="btn ghost sm danger" type="button" disabled={proxyUi.submitting} onclick={() => remove(job.key)}>Отменить</button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if proxyUi?.list?.proxies.length}
    <div class="proxy-group">
      <h3>Готовы</h3>
      <ul>
        {#each proxyUi.list.proxies as artifact (artifact.key)}
          <li>
            <div class="proxy-item-main">
              <strong>{formatProxyProfile(artifact.profile)}</strong>
              <span>{fmtSize(artifact.sizeBytes)}</span>
            </div>
            <button
              class="btn ghost sm"
              class:active={previewAvailable(artifact.profile.codec) && preference?.mode === 'proxy' && preference.key === artifact.key && proxyUi.failedKey !== artifact.key}
              type="button"
              aria-pressed={previewAvailable(artifact.profile.codec) && preference?.mode === 'proxy' && preference.key === artifact.key && proxyUi.failedKey !== artifact.key}
              disabled={!previewAvailable(artifact.profile.codec)}
              title={previewAvailable(artifact.profile.codec) ? '' : 'ProRes Proxy недоступен для просмотра на этой платформе'}
              onclick={() => controller.setProxy(entry.id, artifact.key)}
            >{!previewAvailable(artifact.profile.codec) ? 'Недоступно' : preference?.mode === 'proxy' && preference.key === artifact.key && proxyUi.failedKey !== artifact.key ? 'Используется' : 'Для просмотра'}</button>
            <button class="btn ghost sm danger" type="button" disabled={proxyUi.submitting} onclick={() => remove(artifact.key)}>Удалить</button>
          </li>
        {/each}
      </ul>
    </div>
  {:else if proxyUi?.phase === 'ready' && !proxyUi.list?.jobs.length}
    <p class="proxy-note" role="status">Готовых proxy пока нет.</p>
  {/if}
</section>

<style>
  .proxy-controls {
    display: grid;
    gap: 10px;
    flex: 1 0 100%;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .proxy-heading,
  .proxy-original,
  .proxy-group li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .proxy-heading p,
  .proxy-heading strong,
  .proxy-group h3,
  .proxy-note,
  .proxy-error,
  .proxy-warning { margin: 0; }
  .proxy-heading p,
  .proxy-note,
  .proxy-original span,
  .proxy-item-main span {
    color: var(--muted);
    font-size: 11px;
  }
  .proxy-profile {
    display: grid;
    grid-template-columns: minmax(130px, .8fr) minmax(140px, 1fr) auto auto;
    align-items: end;
    gap: 10px;
    min-width: 0;
    margin: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .proxy-profile legend { padding: 0 5px; color: var(--muted); font-size: 12px; }
  .proxy-profile label:not(.proxy-audio) { display: grid; gap: 4px; color: var(--muted); font-size: 11px; }
  .proxy-profile select {
    min-width: 0;
    padding: 7px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    color: var(--text);
  }
  .proxy-audio { display: flex; align-items: center; gap: 6px; min-height: 34px; color: var(--muted); font-size: 12px; }
  .proxy-error { color: var(--danger); font-size: 12px; }
  .proxy-warning { color: var(--warn); font-size: 12px; }
  .proxy-original .active,
  .proxy-group .active { border-color: var(--accent); color: var(--accent); }
  .proxy-group { display: grid; gap: 6px; }
  .proxy-group h3 { color: var(--muted); font-size: 12px; }
  .proxy-group ul { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
  .proxy-group li { padding: 8px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--panel-3); }
  .proxy-item-main { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .proxy-item-main strong { font-size: 12px; }
  .proxy-item-main progress { width: min(240px, 100%); height: 5px; }
  @media (max-width: 760px) {
    .proxy-profile { grid-template-columns: 1fr 1fr; }
  }
  @media (max-width: 480px) {
    .proxy-profile { grid-template-columns: 1fr; }
    .proxy-group li { align-items: flex-start; flex-wrap: wrap; }
  }
</style>
