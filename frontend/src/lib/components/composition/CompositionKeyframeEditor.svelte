<script lang="ts">
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
  import {
    clearCompositionAutomation,
    compositionState,
    deleteCompositionAutomationKeyframe,
    setCompositionAutomationInterpolation,
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
        : COMPOSITION_VISUAL_PROPERTIES,
  )
  $effect(() => {
    if (!properties.includes(property)) property = properties[0]!
  })
  const current = $derived.by(currentValue)
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
    run(() => updateCompositionAutomationKeyframe(target(), property, originalTick, tick, value))
  }

  function addAtPlayhead(): void {
    setCompositionAutomationKeyframe(target(), property)
  }

  function setInterpolation(interpolation: Exclude<CompositionInterpolation, 'ease_in_out_cubic'>): void {
    setCompositionAutomationInterpolation(target(), property, interpolation)
  }

  function clearAnimation(): void {
    clearCompositionAutomation(target(), property)
  }

  function removeKeyframe(tick: number): void {
    deleteCompositionAutomationKeyframe(target(), property, tick)
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
          <option value={candidate}>{labels[candidate]}</option>
        {/each}
      </select>
    </label>
    <label>
      Интерполяция
      <select
        aria-label="Интерполяция keyframes"
        value={track?.interpolation ?? 'linear'}
        disabled={!track}
        onchange={(event) => run(() => setInterpolation(
          event.currentTarget.value as Exclude<CompositionInterpolation, 'ease_in_out_cubic'>,
        ))}
      >
        {#each COMPOSITION_INTERPOLATIONS as interpolation (interpolation)}
          <option value={interpolation}>{interpolation}</option>
        {/each}
      </select>
    </label>
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
      {mode === 'mask' ? 'Сделать статическим' : 'Сбросить параметр'}
    </button>
  </div>

  {#if track}
    <div class="composition-keyframe-table-wrap">
      <table class="composition-keyframe-table">
        <caption>{labels[property]}: {track.keyframes.length} / {MAX_KEYFRAMES_PER_VALUE}</caption>
        <thead><tr><th scope="col">Время, с</th><th scope="col">Значение</th><th scope="col"><span class="visually-hidden">Действие</span></th></tr></thead>
        <tbody>
          {#each track.keyframes as keyframe (keyframe.tick)}
            <tr>
              <td>
                <input
                  aria-label={`Время keyframe ${labels[property]}`}
                  type="number"
                  min="0"
                  max={duration / COMPOSITION_TIME_BASE}
                  step="0.001"
                  value={Number((keyframe.tick / track.timeBase).toFixed(6))}
                  onchange={(event) => updateRow(
                    keyframe.tick,
                    Math.round(inputNumber(event) * track.timeBase),
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
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="composition-help">Параметр пока статический. Поставьте playhead внутри clip и добавьте первый ключ.</p>
  {/if}
</fieldset>
