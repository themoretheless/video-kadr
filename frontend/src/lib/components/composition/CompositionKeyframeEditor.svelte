<script lang="ts">
  import { keyframeLocalTime, keyframeAuthoredTick } from '$lib/composition/keyframeClock'
  import BezierControlEditor from './BezierControlEditor.svelte'
  import { linkedScaleAutomationPatches } from '$lib/state/composition.svelte.js'
  import {
    clipDurationTicks,
    COMPOSITION_AUDIO_PROPERTIES,
    COMPOSITION_INTERPOLATIONS,
    COMPOSITION_MASK_PROPERTIES,
    COMPOSITION_TIME_BASE,
    COMPOSITION_VISUAL_PROPERTIES,
    MAX_KEYFRAMES_PER_VALUE,
    type AudioClip,
    type CompositionAnimatableValue,
    type CompositionAudioProperty,
    type CompositionClip,
    type CompositionInterpolation,
    type CompositionMaskProperty,
    type CompositionVideoMask,
    type CompositionVisualProperty,
    type VideoClip,
  } from '$lib/composition/types.js'
  import {
    audioPropertyBounds,
    audioPropertyValue,
    maskPropertyBounds,
    maskPropertyValue,
    sampleAnimatableValue,
    visualPropertyBounds,
    visualPropertyValue,
  } from '$lib/composition/keyframes.js'
  import { BEZIER_PRESETS, bezierPresetId } from '$lib/composition/easingPresets'
  import { resolvedKeyframeTangents } from '$lib/composition/tangents.js'
  import {
    setCompositionAutomationTangents,
    setCompositionKeyframeSelectionTangentMode,
    editCompositionAutomationTangent,
    clearCompositionAutomation,
    compositionState,
    deleteCompositionAutomationKeyframe,
    deleteCompositionKeyframeSelection,
    selectCompositionKeyframe,
    moveCompositionKeyframeSelection,
    scaleCompositionKeyframeSelection,
    compositionKeyframeSelectionExtent,
    setCompositionAutomationInterpolation,
    setCompositionAutomationExtrapolation,
    setCompositionAutomationKeyframe,
    updateCompositionAutomationKeyframe,
    type CompositionAutomationTarget,
  } from '$lib/state/composition.svelte.js'

  type EditorMode = 'visual' | 'opacity' | 'audio' | 'mask'
  type AutomationProperty = CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty

  let {
    clip,
    mode = 'visual',
    mask,
  }: {
    clip: CompositionClip
    mode?: EditorMode
    mask?: CompositionVideoMask
  } = $props()
  let property = $state<AutomationProperty>('x')
  const selectedTicks = $derived(compositionState.ui.selectedKeyframes.find((subject) => subject.target.clipId === clip.id && subject.target.kind === target().kind && (subject.target.kind !== 'mask' || subject.target.maskId === mask?.id) && subject.property === property)?.ticks ?? [])
  const selectedCount = $derived(compositionState.ui.selectedKeyframes.reduce((count, subject) => count + subject.ticks.length, 0))

  const labels: Record<AutomationProperty, string> = {
    x: 'X',
    y: 'Y',
    scaleX: 'Scale X',
    scaleY: 'Scale Y',
    rotationDegrees: 'Rotation',
    opacity: 'Opacity',
    gain: 'Gain',
    pan: 'Pan',
    width: 'Width',
    height: 'Height',
  }

  const properties = $derived<readonly AutomationProperty[]>(
    mode === 'audio'
      ? COMPOSITION_AUDIO_PROPERTIES
      : mode === 'opacity'
        ? (['opacity'] as const)
      : mode === 'mask'
        ? mask?.shape === 'linear'
          ? (['x', 'y', 'rotationDegrees'] as const)
          : COMPOSITION_MASK_PROPERTIES
        : clip.kind !== 'audio' && clip.scaleLinked === true
          ? COMPOSITION_VISUAL_PROPERTIES.filter((candidate) => candidate !== 'scaleY')
          : COMPOSITION_VISUAL_PROPERTIES,
  )
  $effect(() => {
    if (!properties.includes(property)) property = properties[0]!
  })
  const current = $derived.by(currentValue)
  let controlPreview = $state<CompositionAnimatableValue | undefined>()
  let controlOrigin: typeof compositionState.document | undefined
  let ownedControlPreviews: typeof compositionState.ui.keyframePreviews = []
  function clearControlPreview(): void {
    compositionState.ui.keyframePreviews = compositionState.ui.keyframePreviews.filter((entry) => !ownedControlPreviews.some((owned) => owned.value === entry.value))
    ownedControlPreviews = []; controlPreview = undefined; controlOrigin = undefined
  }
  function previewControl(value: import('$lib/composition/types').CompositionBezierInterpolation): void {
    if (current?.mode !== 'keyframes') return
    const origin = controlOrigin ?? compositionState.document
    clearControlPreview()
    controlOrigin = origin
    controlPreview = { ...current, track: { ...current.track, interpolation: value } }
    const previews = linkedScaleAutomationPatches([{ target: target(), property, value: controlPreview }]).map((patch) => ({ ...patch, value: patch.value!, ticks: [] }))
    const ids = previews.map((entry) => JSON.stringify([entry.target, entry.property]))
    compositionState.ui.keyframePreviews = [...compositionState.ui.keyframePreviews.filter((entry) => !ids.includes(JSON.stringify([entry.target, entry.property]))), ...previews]
    ownedControlPreviews = compositionState.ui.keyframePreviews.filter((entry) => ids.includes(JSON.stringify([entry.target, entry.property])))
  }
  function commitControl(value: import('$lib/composition/types').CompositionBezierInterpolation): void {
    const origin = controlOrigin
    clearControlPreview()
    run(() => {
      if (origin && compositionState.document !== origin) throw new Error('Проект изменился во время правки Bézier')
      setInterpolation(value)
    })
  }
  $effect(() => () => clearControlPreview())
  const track = $derived(current?.mode === 'keyframes' ? current.track : null)

  const duration = $derived(clipDurationTicks(clip))
  const playheadInside = $derived(
    compositionState.transport.playheadTicks >= clip.timelineStartTicks &&
      compositionState.transport.playheadTicks <= clip.timelineStartTicks + duration,
  )
  const bounds = $derived(
    mode === 'audio'
      ? audioPropertyBounds(property as CompositionAudioProperty)
      : mode === 'mask'
        ? maskPropertyBounds(property as CompositionMaskProperty)
        : visualPropertyBounds(property as CompositionVisualProperty),
  )
  const graphPoints = $derived.by(() => {
    const value = resolvedValue()
    const samples = Array.from({ length: 41 }, (_, index) => {
      const localTick = duration * (index / 40)
      return sampleAnimatableValue(value, localTick)
    })
    const minimum = Math.min(...samples)
    const maximum = Math.max(...samples)
    const span = maximum - minimum || 1
    return samples.map((sample, index) => `${(index / 40) * 100},${90 - ((sample - minimum) / span) * 80}`).join(' ')
  })

  function currentValue(): CompositionAnimatableValue | undefined {
    if (mode === 'audio') {
      return isAudioAutomationClip(clip) ? clip.audioAnimation?.[property as CompositionAudioProperty] : undefined
    }
    if (mode === 'mask') return mask?.[property as CompositionMaskProperty]
    return clip.kind === 'audio' ? undefined : clip.animation?.[property as CompositionVisualProperty]
  }

  function resolvedValue(): CompositionAnimatableValue {
    if (controlPreview) return controlPreview
    if (mode === 'audio' && isAudioAutomationClip(clip)) {
      return audioPropertyValue(clip, property as CompositionAudioProperty)
    }
    if (mode === 'mask' && mask) return maskPropertyValue(mask, property as CompositionMaskProperty)
    if (clip.kind !== 'audio') {
      return visualPropertyValue(compositionState.document, clip, property as CompositionVisualProperty)
    }
    return { mode: 'constant', value: 0 }
  }

  function isAudioAutomationClip(candidate: CompositionClip): candidate is VideoClip | AudioClip {
    return candidate.kind === 'video' || candidate.kind === 'audio'
  }

  function run(action: () => void): void {
    try {
      compositionState.ui.message = ''
      action()
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    }
  }

  function inputNumber(event: Event): number {
    return Number((event.currentTarget as HTMLInputElement).value)
  }

  function updateRow(originalTick: number, tick: number, value: number): void {
    run(() => updateCompositionAutomationKeyframe(target(), property, originalTick, tick, value, true))
  }

  function addAtPlayhead(): void {
    setCompositionAutomationKeyframe(target(), property, undefined, undefined, true)
  }

  function setInterpolation(interpolation: Exclude<CompositionInterpolation, 'ease_in_out_cubic'>): void {
    setCompositionAutomationInterpolation(target(), property, interpolation, true)
  }

  function chooseInterpolation(token: string): void {
    if (token.startsWith('preset:')) {
      const preset = BEZIER_PRESETS.find(p => p.id === token.slice(7))
      if (!preset) throw new Error('Unknown easing preset')
      setInterpolation(preset.interpolation)
    } else if (token.startsWith('elastic_')) {
      const previous = typeof track?.interpolation === 'object' && track.interpolation.kind === 'elastic' ? track.interpolation : undefined
      setInterpolation({ kind: 'elastic', dir: token.slice(8) as 'in' | 'out' | 'in_out', amplitude: previous?.amplitude ?? 1, period: previous?.period ?? 0.3 })
    } else if (token === 'bezier') setInterpolation({ kind: 'bezier', p1: [0.25, 0.1], p2: [0.25, 1] })
    else setInterpolation(token as Exclude<CompositionInterpolation, 'ease_in_out_cubic'>)
  }

  function editElastic(parameter: 'amplitude' | 'period', value: number): void {
    if (typeof track?.interpolation !== 'object' || track.interpolation.kind !== 'elastic') return
    setInterpolation({ ...track.interpolation, [parameter]: value })
  }

  function editBezier(point: 'p1' | 'p2', axis: 0 | 1, value: number): void {
    if (typeof track?.interpolation !== 'object' || track.interpolation.kind !== 'bezier') return
    const coordinates: [number, number] = [...track.interpolation[point]]
    coordinates[axis] = value
    setInterpolation({ ...track.interpolation, [point]: coordinates })
  }

  function clearAnimation(): void {
    clearCompositionAutomation(target(), property, true)
  }

  function removeKeyframe(tick: number): void {
    deleteCompositionAutomationKeyframe(target(), property, tick, true)
  }

  function nudgeSelection(direction: -1 | 1): void {
    compositionState.ui.selectedKeyframes = [...moveCompositionKeyframeSelection(compositionState.ui.selectedKeyframes, direction * Math.round(COMPOSITION_TIME_BASE / compositionState.document.canvas.fps))]
  }

  function scaleSelection(factor: number): void {
    const extent = compositionKeyframeSelectionExtent()
    if (!extent || extent.distinct < 2) return
    compositionState.ui.selectedKeyframes = [...scaleCompositionKeyframeSelection(compositionState.ui.selectedKeyframes, extent.first, factor)]
  }

  function target(): CompositionAutomationTarget {
    if (mode === 'mask') {
      if (!mask) throw new Error('Mask editor requires a mask')
      return { kind: 'mask', clipId: clip.id, maskId: mask.id }
    }
    return { kind: mode === 'opacity' ? 'visual' : mode, clipId: clip.id }
  }
</script>

<fieldset class="composition-fieldset composition-keyframe-editor">
  <legend>{mode === 'audio' ? 'Audio keyframes' : mode === 'mask' ? 'Mask keyframes' : mode === 'opacity' ? 'Opacity keyframes' : 'Keyframes'}</legend>
  <div class="composition-form-grid">
    <label>
      Параметр
      <select
        aria-label={`Параметр ${mode === 'visual' ? '' : `${mode} `}keyframes`}
        value={property}
        onchange={(event) => { property = event.currentTarget.value as AutomationProperty }}
      >
        {#each properties as candidate (candidate)}
          <option value={candidate}>{candidate === 'scaleX' && clip.kind !== 'audio' && clip.scaleLinked ? 'Scale' : labels[candidate]}</option>
        {/each}
      </select>
    </label>
    <label>
      Интерполяция
      <select
        aria-label="Интерполяция keyframes"
        value={typeof track?.interpolation === 'object' ? track.interpolation.kind === 'elastic' ? `elastic_${track.interpolation.dir}` : bezierPresetId(track.interpolation) ? `preset:${bezierPresetId(track.interpolation)}` : 'bezier' : track?.interpolation ?? 'linear'}
        disabled={!track}
        onchange={(event) => run(() => chooseInterpolation(event.currentTarget.value))}
      >
        {#each COMPOSITION_INTERPOLATIONS as interpolation (interpolation)}
          <option value={interpolation}>{interpolation}</option>
        {/each}
        <option value="bezier">Bézier</option>
        {#each BEZIER_PRESETS as preset (preset.id)}
          <option value={`preset:${preset.id}`}>{preset.id}</option>
        {/each}
        <option value="elastic_in">Elastic In</option>
        <option value="elastic_out">Elastic Out</option>
        <option value="elastic_in_out">Elastic InOut</option>
      </select>
    </label>
    {#if typeof track?.interpolation === 'object' && track.interpolation.kind === 'elastic'}
      <label>Амплитуда Elastic<input aria-label="Амплитуда Elastic" type="number" min="1" step="0.1" value={track.interpolation.amplitude} onchange={(event) => run(() => editElastic('amplitude', event.currentTarget.valueAsNumber))} /></label>
      <label>Период Elastic<input aria-label="Период Elastic" type="number" min="0" step="0.01" value={track.interpolation.period} onchange={(event) => run(() => editElastic('period', event.currentTarget.valueAsNumber))} /></label>
    {/if}
    {#if typeof track?.interpolation === 'object' && track.interpolation.kind === 'bezier'}
      <BezierControlEditor value={track.interpolation} disabled={Boolean(clip.locked || compositionState.document.tracks.some((lane) => lane.locked && lane.clips.some((candidate) => candidate.id === clip.id)))} onpreview={previewControl} oncommit={commitControl} oncancel={clearControlPreview} />
      {#each ['p1', 'p2'] as point (point)}
        {#each [0, 1] as axis (axis)}
          <label>Bézier {point} {axis === 0 ? 'X' : 'Y'}<input aria-label={`Bézier ${point} ${axis === 0 ? 'X' : 'Y'}`} type="number" min={axis === 0 ? 0 : undefined} max={axis === 0 ? 1 : undefined} step="0.01" value={track.interpolation[point as 'p1' | 'p2'][axis]} onchange={(event) => run(() => editBezier(point as 'p1' | 'p2', axis as 0 | 1, event.currentTarget.valueAsNumber))} /></label>
        {/each}
      {/each}
    {/if}
    {#if track}
      {#each ['before', 'after'] as side (side)}
        <label>
          {side === 'before' ? 'До первого ключа' : 'После последнего ключа'}
          <select aria-label={side === 'before' ? 'Продолжение до ключей' : 'Продолжение после ключей'} value={track.extrapolation?.[side as 'before' | 'after'] ?? 'hold'} onchange={(event) => run(() => setCompositionAutomationExtrapolation(target(), property, side as 'before' | 'after', event.currentTarget.value as 'hold' | 'continue' | 'loop' | 'ping_pong' | 'offset', true))}>
            <option value="hold">Удерживать значение</option>
            <option value="continue">Продолжить наклон</option>
            <option value="loop">Повторять</option>
            <option value="ping_pong">Туда и обратно</option>
            <option value="offset">Повторять со смещением</option>
          </select>
        </label>
      {/each}
    {/if}
  </div>

  <svg class="composition-keyframe-graph" viewBox="0 0 100 100" role="img" aria-label={`График ${labels[property]} по времени`} preserveAspectRatio="none">
    <line x1="0" y1="90" x2="100" y2="90"></line>
    <polyline points={graphPoints}></polyline>
  </svg>

  <div class="composition-inspector-actions">
    <button
      class="btn ghost sm"
      type="button"
      disabled={!playheadInside || (track?.keyframes.length ?? 0) >= MAX_KEYFRAMES_PER_VALUE}
      title={playheadInside ? 'Добавить или обновить ключ на текущем playhead' : 'Переместите playhead внутрь clip'}
      onclick={() => run(addAtPlayhead)}
    >
      + Ключ на playhead
    </button>
    <button class="btn ghost sm" type="button" disabled={mode === 'mask' ? !track : !current} onclick={() => run(clearAnimation)}>
      Сделать статическим
    </button>
  </div>

  {#if track}
    {#if selectedCount}
      <div class="composition-actions">
        <button class="btn ghost sm" type="button" onclick={() => run(() => setCompositionKeyframeSelectionTangentMode('auto'))}>Auto выбранных ключей</button>
        <button class="btn ghost sm" type="button" onclick={() => run(() => setCompositionKeyframeSelectionTangentMode('free'))}>Free выбранных ключей</button>
        <button class="btn ghost sm" type="button" aria-label="Сдвинуть выбранные ключи назад" onclick={() => run(() => nudgeSelection(-1))}>←</button>
        <button class="btn ghost sm" type="button" aria-label="Сдвинуть выбранные ключи вперёд" onclick={() => run(() => nudgeSelection(1))}>→</button>
        <button class="btn ghost sm" type="button" disabled={selectedCount < 2} onclick={() => run(() => scaleSelection(0.5))}>Сжать время ×0.5</button>
        <button class="btn ghost sm" type="button" disabled={selectedCount < 2} onclick={() => run(() => scaleSelection(2))}>Растянуть время ×2</button>
        <button class="btn ghost sm danger" type="button" onclick={() => run(() => { deleteCompositionKeyframeSelection(compositionState.ui.selectedKeyframes); compositionState.ui.selectedKeyframes = [] })}>Удалить выбранные ключи ({selectedCount})</button>
      </div>
    {/if}
    <div class="composition-keyframe-table-wrap">
      <table class="composition-keyframe-table">
        <caption>{labels[property]}: {track.keyframes.length} / {MAX_KEYFRAMES_PER_VALUE}</caption>
        <thead><tr><th scope="col">Выбор</th><th scope="col">Время, с</th><th scope="col">Значение</th><th scope="col"><span class="visually-hidden">Действие</span></th></tr></thead>
        <tbody>
          {#each track.keyframes as keyframe, index (keyframe.tick)}
            <tr>
              <td><input type="checkbox" aria-label={`Выбрать keyframe ${labels[property]} ${keyframe.tick}`} checked={selectedTicks.includes(keyframe.tick)} onchange={() => selectCompositionKeyframe(target(), property, keyframe.tick, true)} /></td>
              <td>
                <input
                  aria-label={`Время keyframe ${labels[property]}`}
                  type="number"
                  step="0.001"
                  value={Number((keyframeLocalTime(track, keyframe.tick, 1)).toFixed(6))}
                  onchange={(event) => updateRow(
                    keyframe.tick,
                    keyframeAuthoredTick(track, inputNumber(event), 1),
                    keyframe.value,
                  )}
                />
              </td>
              <td>
                <input
                  aria-label={`Значение keyframe ${labels[property]}`}
                  type="number"
                  min={bounds.minimum}
                  max={bounds.maximum}
                  step={bounds.step}
                  value={keyframe.value}
                  onchange={(event) => updateRow(keyframe.tick, keyframe.tick, inputNumber(event))}
                />
              </td>
              <td><button class="btn ghost sm danger" type="button" aria-label={`Удалить keyframe ${labels[property]} ${keyframe.tick}`} onclick={() => run(() => removeKeyframe(keyframe.tick))}>×</button></td>
            </tr>
            {@const segment = keyframe.interpolation ?? track.interpolation}
            {@const arrivingSegment = index > 0 ? track.keyframes[index - 1]!.interpolation ?? track.interpolation : undefined}
            {#if keyframe.tangents || (typeof segment === 'object' && segment.kind === 'bezier') || (arrivingSegment && typeof arrivingSegment === 'object' && arrivingSegment.kind === 'bezier')}
              {@const tangents = resolvedKeyframeTangents(track.keyframes, index, track.interpolation)}
              <tr><td colspan="4">
                <details><summary>Касательные {keyframeLocalTime(track, keyframe.tick, 1)} с</summary>
                  {#each ['incoming','outgoing'] as side (side)}
                    <label>{side === 'incoming' ? 'Входящая' : 'Исходящая'}
                      <select aria-label={`Режим ${side} keyframe ${keyframe.tick}`} value={tangents[side as 'incoming' | 'outgoing'].mode} onchange={(event) => run(() => setCompositionAutomationTangents(target(), property, [keyframe.tick], { [side]: { ...tangents[side as 'incoming' | 'outgoing'], mode: event.currentTarget.value as 'auto' | 'free' } }, true))}>
                        <option value="free">Free</option><option value="auto">Auto</option>
                      </select>
                    </label>
                    {#each ['x','y'] as axis (axis)}
                      <label>{axis.toUpperCase()}<input type="number" step="0.01" min={axis === 'x' ? 0 : undefined} max={axis === 'x' ? 1 : undefined} aria-label={`Касательная ${side} ${axis} keyframe ${keyframe.tick}`} value={tangents[side as 'incoming' | 'outgoing'][axis as 'x' | 'y']} onchange={(event) => run(() => editCompositionAutomationTangent(target(), property, keyframe.tick, side as 'incoming' | 'outgoing', axis === 'x' ? event.currentTarget.valueAsNumber : tangents[side as 'incoming' | 'outgoing'].x, axis === 'y' ? event.currentTarget.valueAsNumber : tangents[side as 'incoming' | 'outgoing'].y, true))} /></label>
                    {/each}
                  {/each}
                  <label>Непрерывность
                    <select aria-label={`Непрерывность keyframe ${keyframe.tick}`} value={tangents.continuity} onchange={(event) => run(() => setCompositionAutomationTangents(target(), property, [keyframe.tick], { continuity: event.currentTarget.value as 'smooth' | 'broken' }, true))}>
                      <option value="broken">Broken</option><option value="smooth">Smooth</option>
                    </select>
                  </label>
                </details>
              </td></tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="composition-help">Параметр пока статический. Поставьте playhead внутри clip и добавьте первый ключ.</p>
  {/if}
</fieldset>
