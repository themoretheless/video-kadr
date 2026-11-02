<script lang="ts">
  import InterpolationNumbers from './InterpolationNumbers.svelte'
  import ColorCurveEditor from './ColorCurveEditor.svelte'
  import type { TextClip,ColorClip } from '$lib/composition/types'
  import { clipDurationTicks } from '$lib/composition/types'
  import { setTextColorKey,deleteTextColorKey,updateTextColorKey,configureTextColor,setTextColorTangentMode,editTextColorTangents } from '$lib/composition/colorCommands'
  import { resolvedColorTangents } from '$lib/composition/colorAnimation'
  import { EASING_PRESETS,easingPresetId } from '$lib/composition/easingPresets'
  import { segmentInterpolation } from '$lib/composition/tangents'
  import { compositionState,commitDocument } from '$lib/state/composition/core.svelte'
  let { clip }: { clip: TextClip | ColorClip } = $props()
  const current = $derived.by(() => {
    for (const track of compositionState.document.tracks) {
      const candidate = track.clips.find(item => item.id === clip.id)
      if (candidate?.kind === 'text' || candidate?.kind === 'color') return candidate
    }
    return clip
  })
  const locked = $derived(Boolean(current.locked || compositionState.document.tracks.some(track => track.locked && track.clips.some(item => item.id === current.id))))
  const outside = $derived(compositionState.transport.playheadTicks < current.timelineStartTicks || compositionState.transport.playheadTicks > current.timelineStartTicks+clipDurationTicks(current))
  let draft = $state('')
  let draftClipId = ''
  $effect(() => {
    if (draftClipId !== current.id) {
      draftClipId = current.id
      draft = current.colorAnimation?.mode === 'constant' ? current.colorAnimation.value : current.kind === 'text' ? current.style.color : current.color
    }
  })
  function run(action: () => typeof compositionState.document) {
    if (locked) return
    try { commitDocument(action()) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
</script>
<fieldset class="composition-inspector-group" disabled={locked}>
  <legend>Анимация цвета</legend>
  <label>Цвет ключа<input class="composition-color-field" aria-label="Цвет нового ключа" bind:value={draft} /></label>
  <button class="btn ghost sm" disabled={outside} onclick={() => run(() => setTextColorKey(compositionState.document,current.id,compositionState.transport.playheadTicks,draft))}>Ключ цвета на текущем кадре</button>
  {#if current.colorAnimation?.mode === 'keyframes'}
    {@const track = current.colorAnimation.track}
    <label>Кривая цвета<select aria-label="Кривая цветовой дорожки" value={easingPresetId(track.interpolation) ?? ''} onchange={event => run(() => configureTextColor(compositionState.document,current.id,{ preset: event.currentTarget.value }))}>
      <option value="" disabled>Пользовательская</option>
      {#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}
    </select></label>
    <InterpolationNumbers value={track.interpolation} label="Кривая цвета" onChange={interpolation => run(() => configureTextColor(compositionState.document,current.id,{ interpolation }))} />
    {#if typeof track.interpolation === 'object' && track.interpolation.kind === 'bezier'}
      <ColorCurveEditor clipId={current.id} value={track.interpolation} disabled={locked} label="График цветовой дорожки" />
    {/if}
    {#each ['before','after'] as side (side)}
      <label>{side === 'before' ? 'До первого ключа' : 'После последнего ключа'}<select aria-label={`Экстраполяция цвета ${side}`} value={track.extrapolation?.[side as 'before' | 'after'] ?? 'hold'} onchange={event => run(() => configureTextColor(compositionState.document,current.id,{ side: side as 'before' | 'after',extrapolation: event.currentTarget.value as import('$lib/composition/types').CompositionExtrapolationMode }))}>
        {#each ['hold','continue','loop','ping_pong','offset'] as mode (mode)}<option value={mode}>{mode}</option>{/each}
      </select></label>
    {/each}
    {#each track.keyframes as key,index (key.tick)}
      {@const tangents = resolvedColorTangents({ mode: 'keyframes',track },index)}
      <div class="composition-form-grid">
        <label>Время, с<input aria-label={`Время цветового ключа ${key.tick}`} type="number" step={1/track.timeBase} value={key.tick/track.timeBase} onchange={event => run(() => updateTextColorKey(compositionState.document,current.id,key.tick,{ tick: Math.round(Number(event.currentTarget.value)*track.timeBase) }))} /></label>
        <label>Цвет<input class="composition-color-field" aria-label={`Цвет ключа ${key.tick}`} value={key.value} onchange={event => run(() => updateTextColorKey(compositionState.document,current.id,key.tick,{ value: event.currentTarget.value }))} /></label>
        <button class="btn ghost sm" aria-label={`Auto цветового ключа ${key.tick}`} onclick={() => run(() => setTextColorTangentMode(compositionState.document,current.id,key.tick,'auto'))}>Auto</button>
        <button class="btn ghost sm" aria-label={`Free цветового ключа ${key.tick}`} onclick={() => run(() => setTextColorTangentMode(compositionState.document,current.id,key.tick,'free'))}>Free</button>
        <label>Связь ручек<select aria-label={`Связь ручек цвета ${key.tick}`} value={tangents.continuity} onchange={event => run(() => editTextColorTangents(compositionState.document,current.id,key.tick,{ continuity: event.currentTarget.value as 'smooth' | 'broken' }))}><option value="smooth">Smooth</option><option value="broken">Broken</option></select></label>
        <details><summary>Координаты ручек</summary>
          {#each ['incoming','outgoing'] as side (side)}
            {#each ['x','y'] as axis (axis)}
              <label>{side} {axis}<input aria-label={`Ручка цвета ${key.tick} ${side} ${axis}`} type="number" step="0.01" min={axis === 'x' ? 0 : undefined} max={axis === 'x' ? 1 : undefined} value={tangents[side as 'incoming' | 'outgoing'][axis as 'x' | 'y']} onchange={event => run(() => editTextColorTangents(compositionState.document,current.id,key.tick,{ side: side as 'incoming' | 'outgoing',point: { x: tangents[side as 'incoming' | 'outgoing'].x,y: tangents[side as 'incoming' | 'outgoing'].y,[axis]: Number(event.currentTarget.value) } }))} /></label>
            {/each}
          {/each}
        </details>
        {#if track.keyframes[index+1]}
          {@const mode = segmentInterpolation({ ...key,value: 0 },{ ...track.keyframes[index+1]!,value: 1 },track.interpolation)}
          <label>Сегмент<select aria-label={`Кривая цветового ключа ${key.tick}`} value={easingPresetId(mode) ?? ''} onchange={event => run(() => configureTextColor(compositionState.document,current.id,{ tick: key.tick,preset: event.currentTarget.value }))}>
            <option value="" disabled>Пользовательская</option>
            {#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}
          </select></label>
          <InterpolationNumbers value={mode} label={`Цветовой сегмент ${key.tick}`} onChange={interpolation => run(() => configureTextColor(compositionState.document,current.id,{ tick: key.tick,interpolation }))} />
          {#if typeof mode === 'object' && mode.kind === 'bezier'}
            <details><summary>График сегмента</summary><ColorCurveEditor clipId={current.id} tick={key.tick} value={mode} disabled={locked} label={`График цветового сегмента ${key.tick}`} /></details>
          {/if}
        {/if}
        <button class="btn ghost sm" aria-label={`Удалить цветовой ключ ${key.tick}`} onclick={() => run(() => deleteTextColorKey(compositionState.document,current.id,key.tick))}>Удалить ключ</button>
      </div>
    {/each}
  {/if}
</fieldset>
