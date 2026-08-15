<script lang="ts">
  import { localWaveformCache } from '$lib/audio/waveformCache.js'
  import {
    COMPOSITION_TIME_BASE,
    type CompositionSource,
  } from '$lib/composition/types.js'
  import type { MulticamAngle, MulticamGroup } from '$lib/composition/multicam.js'
  import {
    compositionMulticamGroups,
    compositionState,
    createCompositionMulticamGroup,
    estimateCompositionMulticamSync,
    rebuildCompositionMulticamGroup,
    recordCompositionMulticamSwitch,
    type MulticamWaveformCachePort,
  } from '$lib/state/composition.svelte.js'
  import CompositionMediaLayer from './CompositionMediaLayer.svelte'

  interface Props {
    waveformCache?: MulticamWaveformCachePort
  }

  let { waveformCache = localWaveformCache }: Props = $props()
  let selectedSourceIds = $state<string[]>([])
  let labels = $state<Record<string, string>>({})
  let offsetsSeconds = $state<Record<string, number>>({})
  let masterAudioSourceId = $state('')
  let groupName = $state('Multicam')
  let groupStartSeconds = $state(0)
  let syncBusy = $state(false)
  let localMessage = $state('')
  let localError = $state('')

  const sources = $derived(
    Object.values(compositionState.document.sources)
      .filter((source): source is CompositionSource & { kind: 'video' } => source.kind === 'video')
      .sort((left, right) => left.id.localeCompare(right.id)),
  )
  const groups = $derived(compositionMulticamGroups())
  const selectedGroup = $derived(
    groups.find((group) => group.id === compositionState.ui.selectedMulticamGroupId) ?? groups[0] ?? null,
  )
  const syncReason = $derived.by(() => {
    if (selectedSourceIds.length < 2) return 'Выберите минимум 2 angles'
    const unavailable = selectedSourceIds.find((id) => {
      const source = compositionState.document.sources[id]
      return !source?.hasAudio || !compositionState.media[id]?.url
    })
    return unavailable ? `Для ${sourceLabel(unavailable)} недоступен локальный audio waveform` : ''
  })

  $effect(() => {
    const available = new Set(sources.map((source) => source.id))
    const filtered = selectedSourceIds.filter((id) => available.has(id)).slice(0, 8)
    if (filtered.join('\0') !== selectedSourceIds.join('\0')) selectedSourceIds = filtered
    if (masterAudioSourceId && !filtered.includes(masterAudioSourceId)) masterAudioSourceId = ''
  })

  $effect(() => {
    if (selectedGroup && compositionState.ui.selectedMulticamGroupId !== selectedGroup.id) {
      compositionState.ui.selectedMulticamGroupId = selectedGroup.id
    }
  })

  function sourceLabel(sourceId: string): string {
    const media = compositionState.media[sourceId]
    return media?.filename || sourceId
  }

  function toggleSource(source: CompositionSource, selected: boolean): void {
    if (selected) {
      if (selectedSourceIds.length >= 8) return
      selectedSourceIds = [...selectedSourceIds, source.id]
      labels[source.id] ??= sourceLabel(source.id)
      offsetsSeconds[source.id] ??= 0
      if (!masterAudioSourceId && source.hasAudio) masterAudioSourceId = source.id
    } else {
      selectedSourceIds = selectedSourceIds.filter((id) => id !== source.id)
      if (masterAudioSourceId === source.id) {
        masterAudioSourceId = selectedSourceIds.find((id) => compositionState.document.sources[id]?.hasAudio) ?? ''
      }
    }
  }

  async function synchronize(): Promise<void> {
    if (syncBusy || syncReason) return
    syncBusy = true
    localError = ''
    localMessage = ''
    try {
      const estimates = await estimateCompositionMulticamSync(selectedSourceIds, waveformCache)
      for (const estimate of estimates) {
        offsetsSeconds[estimate.sourceId] = estimate.sourceTickAtGroupStart / COMPOSITION_TIME_BASE
      }
      const weakest = Math.min(...estimates.map((estimate) => estimate.confidence))
      localMessage = `Waveform sync готов · минимальная confidence ${(weakest * 100).toFixed(0)}%`
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error)
    } finally {
      syncBusy = false
    }
  }

  function createGroup(): void {
    localError = ''
    localMessage = ''
    try {
      const id = createCompositionMulticamGroup({
        name: groupName,
        timelineStartTicks: Math.max(0, Math.round(groupStartSeconds * COMPOSITION_TIME_BASE)),
        audioSourceId: masterAudioSourceId,
        angles: selectedSourceIds.map((sourceId) => ({
          sourceId,
          label: labels[sourceId]?.trim() || sourceLabel(sourceId),
          sourceTickAtGroupStart: Math.max(0, Math.round((offsetsSeconds[sourceId] ?? 0) * COMPOSITION_TIME_BASE)),
        })),
      })
      compositionState.ui.selectedMulticamGroupId = id
      localMessage = 'Multicam group создана и скомпилирована в ordinary clips'
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error)
    }
  }

  function selectGroup(event: Event): void {
    compositionState.ui.selectedMulticamGroupId = (event.currentTarget as HTMLSelectElement).value
  }

  function activeAngleId(group: MulticamGroup): string | null {
    const playhead = compositionState.transport.playheadTicks
    return [...group.switches]
      .filter((change) => change.timelineTick <= playhead)
      .sort((left, right) => right.timelineTick - left.timelineTick || left.id.localeCompare(right.id))[0]?.angleId ?? null
  }

  function groupContainsPlayhead(group: MulticamGroup): boolean {
    const playhead = compositionState.transport.playheadTicks
    return playhead >= group.timelineStartTicks && playhead < group.timelineStartTicks + group.durationTicks
  }

  function groupLockReason(group: MulticamGroup): string {
    const video = compositionState.document.tracks.find((track) => track.id === group.videoTrackId)
    const audio = group.audioTrackId
      ? compositionState.document.tracks.find((track) => track.id === group.audioTrackId)
      : undefined
    if (!video || video.kind !== 'video') return 'Program video track недоступна'
    if (video.locked) return 'Program video track заблокирована'
    if (group.audioTrackId && (!audio || audio.kind !== 'audio')) return 'Master audio track недоступна'
    if (audio?.locked) return 'Master audio track заблокирована'
    return ''
  }

  function angleSourceTime(group: MulticamGroup, angle: MulticamAngle): number {
    const local = Math.max(0, Math.min(group.durationTicks - 1, compositionState.transport.playheadTicks - group.timelineStartTicks))
    return (angle.sourceTickAtGroupStart + local) / COMPOSITION_TIME_BASE
  }

  function recordAngle(group: MulticamGroup, angleId: string): void {
    localError = ''
    localMessage = ''
    try {
      recordCompositionMulticamSwitch(group.id, angleId)
      localMessage = `Switch записан на ${(compositionState.transport.playheadTicks / COMPOSITION_TIME_BASE).toFixed(3)} с`
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error)
    }
  }

  function rebuild(group: MulticamGroup): void {
    localError = ''
    localMessage = ''
    try {
      rebuildCompositionMulticamGroup(group.id)
      localMessage = 'Program cuts и master audio пересобраны'
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error)
    }
  }

  function formatSeconds(ticks: number): string {
    return (ticks / COMPOSITION_TIME_BASE).toFixed(3)
  }
</script>

<details class="composition-tool-section" aria-label="Multicam">
  <summary>Multicam</summary>
  <div class="composition-tool-body multicam-tool">
    <p>Выберите 2–8 уже зарегистрированных video sources. Waveform sync выполняется локально; backend получает только обычные program cuts и master audio.</p>

    {#if sources.length >= 2}
      <fieldset class="multicam-source-picker">
        <legend>Angles</legend>
        {#each sources as source (source.id)}
          {@const selected = selectedSourceIds.includes(source.id)}
          <section class:active={selected} class="multicam-source-row">
            <label class="composition-check">
              <input
                aria-label={`Выбрать angle ${sourceLabel(source.id)}`}
                type="checkbox"
                checked={selected}
                disabled={!selected && selectedSourceIds.length >= 8}
                onchange={(event) => toggleSource(source, event.currentTarget.checked)}
              />
              {sourceLabel(source.id)} {source.hasAudio ? '· audio' : '· silent'}
            </label>
            {#if selected}
              <label>Label<input aria-label={`Label angle ${source.id}`} maxlength="128" bind:value={labels[source.id]} /></label>
              <label>Source offset, с<input aria-label={`Offset angle ${source.id}`} type="number" min="0" step="0.001" bind:value={offsetsSeconds[source.id]} /></label>
            {/if}
          </section>
        {/each}
      </fieldset>
      <div class="composition-form-grid">
        <label>Название<input aria-label="Название multicam group" maxlength="128" bind:value={groupName} /></label>
        <label>Начало, с<input aria-label="Начало multicam group" type="number" min="0" step="0.001" bind:value={groupStartSeconds} /></label>
        <label>Master audio angle
          <select aria-label="Master audio angle" bind:value={masterAudioSourceId}>
            <option value="">Выберите angle…</option>
            {#each selectedSourceIds.filter((id) => compositionState.document.sources[id]?.hasAudio) as sourceId (sourceId)}
              <option value={sourceId}>{labels[sourceId] || sourceLabel(sourceId)}</option>
            {/each}
          </select>
        </label>
      </div>
      <div class="composition-tool-actions">
        <button class="btn ghost sm" type="button" disabled={Boolean(syncReason) || syncBusy} title={syncReason} onclick={() => void synchronize()}>
          {syncBusy ? 'Синхронизирую…' : 'Синхронизировать по waveform'}
        </button>
        <button class="btn primary sm" type="button" disabled={selectedSourceIds.length < 2 || !masterAudioSourceId} onclick={createGroup}>Создать multicam group</button>
      </div>
      {#if syncReason}<small class="composition-help">{syncReason}</small>{/if}
    {:else}
      <p>Добавьте в композицию минимум два video sources.</p>
    {/if}

    {#if groups.length}
      <hr />
      <label>Группа
        <select aria-label="Multicam group" value={selectedGroup?.id ?? ''} onchange={selectGroup}>
          {#each groups as group (group.id)}<option value={group.id}>{group.name}</option>{/each}
        </select>
      </label>
    {/if}

    {#if selectedGroup}
      {@const lockReason = groupLockReason(selectedGroup)}
      <div class="multicam-viewer" role="group" aria-label={`Angles ${selectedGroup.name}`}>
        {#each selectedGroup.angles as angle (angle.id)}
          {@const media = compositionState.media[angle.sourceId]}
          <button
            class:active={activeAngleId(selectedGroup) === angle.id}
            class="multicam-angle"
            type="button"
            aria-pressed={activeAngleId(selectedGroup) === angle.id}
            aria-label={`Переключить на angle ${angle.label}`}
            disabled={!groupContainsPlayhead(selectedGroup) || Boolean(lockReason)}
            title={lockReason || (!groupContainsPlayhead(selectedGroup) ? 'Плейхед вне multicam group' : 'Записать switch на playhead')}
            onclick={() => recordAngle(selectedGroup, angle.id)}
          >
            {#if media}
              <CompositionMediaLayer
                kind="video"
                url={media.url}
                sourceTime={angleSourceTime(selectedGroup, angle)}
                playing={compositionState.transport.playing && groupContainsPlayhead(selectedGroup)}
                muted={true}
                class="multicam-angle-video"
              />
            {:else}
              <span class="multicam-angle-missing">Media offline</span>
            {/if}
            <strong>{angle.label}</strong>
            <small>offset {formatSeconds(angle.sourceTickAtGroupStart)} с</small>
          </button>
        {/each}
      </div>
      <p>{compositionState.transport.playing ? '● Live switching: клик записывает EDL на текущем playhead.' : 'Клик по angle добавляет или заменяет switch на playhead.'}</p>
      {#if lockReason}<p class="error" role="alert">{lockReason}</p>{/if}
      <div class="composition-tool-actions">
        <button class="btn ghost sm" type="button" disabled={Boolean(lockReason)} onclick={() => rebuild(selectedGroup)}>Пересобрать owned cuts</button>
        <span>{selectedGroup.switches.length} switches · {formatSeconds(selectedGroup.durationTicks)} с</span>
      </div>
      <div class="multicam-edl-wrap">
        <table class="multicam-edl">
          <caption>Switch EDL</caption>
          <thead><tr><th>Timeline</th><th>Angle</th><th>Clip ID</th></tr></thead>
          <tbody>
            {#each selectedGroup.switches as change (change.id)}
              <tr>
                <td>{formatSeconds(change.timelineTick)} с</td>
                <td>{selectedGroup.angles.find((angle) => angle.id === change.angleId)?.label ?? change.angleId}</td>
                <td><code>{change.clipId}</code></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    {#if localMessage}<p class="composition-local-message" role="status">{localMessage}</p>{/if}
    {#if localError}<p class="error" role="alert">{localError}</p>{/if}
  </div>
</details>
