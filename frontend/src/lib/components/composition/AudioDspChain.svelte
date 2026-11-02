<script lang="ts">
  import { getAudioDspCatalog, audioDspEditorEffects, type NativeAudioDspEffect } from '$lib/api'
  import { upsertDspKey, setDspInterpolation, editDspKey, removeDspKey } from '$lib/composition/dsp'
  import type { CompositionDspEffect, CompositionSimpleInterpolation } from '$lib/composition/types'
  import AudioSourceParamKeys from './AudioSourceParamKeys.svelte'
  import AudioProjectResponse from './AudioProjectResponse.svelte'
  let { effects = [], localTick = 0, onchange }: { effects?: readonly CompositionDspEffect[]; localTick?: number; onchange: (effects: readonly CompositionDspEffect[]) => void } = $props()
  let catalog = $state<readonly NativeAudioDspEffect[]>([])
  let error = $state('')
  $effect(() => {
    const controller = new AbortController()
    getAudioDspCatalog(controller.signal).then(value => {
      if (!controller.signal.aborted) catalog = audioDspEditorEffects(value)
    }).catch(reason => { if (!controller.signal.aborted) error = String(reason) })
    return () => controller.abort()
  })
  function patch(index: number, update: Partial<CompositionDspEffect>) {
    onchange(effects.map((effect, i) => i === index ? { ...effect, ...update } : effect))
  }
  function parameter(index: number, id: string, value: number) {
    if (!Number.isFinite(value)) return
    if (effects[index]?.animation?.[id]) key(index, id, value)
    else patch(index, { params: { ...effects[index]?.params, [id]: value } })
  }
  function key(index: number, id: string, value: number) {
    onchange(effects.map((effect, i) => i === index ? upsertDspKey(effect, id, localTick, value) : effect))
  }
  function clearCurve(index: number, id: string) {
    const animation = { ...effects[index]?.animation }; delete animation[id]
    patch(index, { animation })
  }
  function move(index: number, direction: number) {
    const next = [...effects]; const target = index + direction
    if (target < 0 || target >= next.length) return
    ;[next[index], next[target]] = [next[target]!, next[index]!]
    onchange(next)
  }
</script>
<div class="composition-dsp-chain">
  <label>DSP эффект
    <select aria-label="Добавить DSP эффект" value="" disabled={effects.length >= 16 || !catalog.length} onchange={event => {
      if (event.currentTarget.value) onchange([...effects, { id: event.currentTarget.value }])
      event.currentTarget.value = ''
    }}>
      <option value="">Добавить…</option>
      {#each catalog.filter(effect => effect.nativeExport) as effect (effect.id)}
        <option value={effect.id}>{effect.name}</option>
      {/each}
    </select>
  </label>
  {#if error}<p role="status">{error}</p>{/if}
  {#each effects as effect, index (index)}
    {@const info = catalog.find(entry => entry.id === effect.id)}
    <div>
      <span>{info?.name ?? effect.id}</span>
      <label><input type="checkbox" aria-label={`Bypass DSP ${index + 1}`} checked={effect.bypass ?? false} onchange={event => patch(index, { bypass: event.currentTarget.checked })} /> Bypass</label>
      <button type="button" aria-label={`DSP ${index + 1} вверх`} disabled={index === 0} onclick={() => move(index, -1)}>↑</button>
      <button type="button" aria-label={`DSP ${index + 1} вниз`} disabled={index === effects.length - 1} onclick={() => move(index, 1)}>↓</button>
      <button type="button" aria-label={`Удалить DSP ${index + 1}`} onclick={() => onchange(effects.filter((_, i) => i !== index))}>Удалить</button>
      {#each info?.params ?? [] as param (param.id)}
        <label>{param.name}
          {#if param.unit === 'choice' || param.unit === 'toggle'}
            <select aria-label={`DSP ${index + 1} ${param.name}`} value={effect.params?.[param.id] ?? param.default} onchange={event => parameter(index, param.id, Number(event.currentTarget.value))}>
              {#each param.unit === 'toggle' ? ['Off', 'On'] : param.choices as name, choice}<option value={choice}>{name}</option>{/each}
            </select>
          {:else}
            <input aria-label={`DSP ${index + 1} ${param.name}`} type="number" min={param.min} max={param.max} step={param.step ?? 'any'} value={effect.params?.[param.id] ?? param.default} onchange={event => {
              const value = event.currentTarget.valueAsNumber
              if (value >= param.min && value <= param.max) parameter(index, param.id, value)
            }} />
          {/if}
        </label>
        {#if effect.id.startsWith('concat.')}
          <AudioSourceParamKeys keys={effect.concatSourceKeys?.[param.id]??[]} rest={effect.params?.[param.id]??param.default} label={`DSP ${index+1} ${param.name}`} onchange={keys=>{const concatSourceKeys={...effect.concatSourceKeys};if(keys.length)concatSourceKeys[param.id]=keys;else delete concatSourceKeys[param.id];patch(index,{concatSourceKeys})}} />
        {/if}
        <button type="button" aria-label={`Ключ DSP ${index + 1} ${param.name}`} disabled={param.animatable === false} onclick={() => key(index, param.id, effect.params?.[param.id] ?? param.default)}>Ключ на курсоре</button>
        {#if effect.animation?.[param.id]}
          <button type="button" aria-label={`Убрать кривую DSP ${index + 1} ${param.name}`} onclick={() => clearCurve(index, param.id)}>Убрать кривую</button>
          {@const curve = effect.animation?.[param.id]}
          {#if curve?.mode === 'keyframes'}
            <label>Интерполяция<select aria-label={`Интерполяция DSP ${index + 1} ${param.name}`} value={typeof curve.track.interpolation === 'string' ? curve.track.interpolation : 'custom'} onchange={event => {
              const interpolation = event.currentTarget.value as CompositionSimpleInterpolation
              onchange(effects.map((entry,i) => i === index ? setDspInterpolation(entry,param.id,interpolation) : entry))
            }}>
              {#if typeof curve.track.interpolation !== 'string'}<option value="custom" disabled>Авторская кривая</option>{/if}
              {#each ['hold','linear','ease_in','ease_out','ease_in_out','ease_in_out_cubic','bounce_in','bounce_out','bounce_in_out'] as mode}<option value={mode}>{mode}</option>{/each}
            </select></label>
            <details>
              <summary>Ключи ({curve.track.keyframes.length})</summary>
              <div class="dsp-keys">
                {#each curve.track.keyframes as point, keyIndex (point.tick)}
                  <div class="dsp-key">
                    <label>Время, с<input aria-label={`DSP ${index + 1} ${param.name} ключ ${keyIndex + 1} время`} type="number" step="any" value={point.tick / curve.track.timeBase} onchange={event => {
                      const seconds = event.currentTarget.valueAsNumber
                      if (Number.isFinite(seconds)) onchange(effects.map((entry,i) => i === index ? editDspKey(entry,param.id,keyIndex,{ localTick: seconds * 1000000 }) : entry))
                    }} /></label>
                    <label>Значение<input aria-label={`DSP ${index + 1} ${param.name} ключ ${keyIndex + 1} значение`} type="number" min={param.min} max={param.max} step={param.unit === 'choice' || param.unit === 'toggle' ? 1 : 'any'} value={point.value} onchange={event => {
                      const value = event.currentTarget.valueAsNumber
                      if (value >= param.min && value <= param.max && Number.isFinite(value) && (!(param.unit === 'choice' || param.unit === 'toggle') || Number.isInteger(value))) onchange(effects.map((entry,i) => i === index ? editDspKey(entry,param.id,keyIndex,{ value }) : entry))
                    }} /></label>
                    <button type="button" aria-label={`Удалить ключ DSP ${index + 1} ${param.name} ${keyIndex + 1}`} onclick={() => onchange(effects.map((entry,i) => i === index ? removeDspKey(entry,param.id,keyIndex) : entry))}>Удалить ключ</button>
                  </div>
                {/each}
              </div>
            </details>
          {/if}
          <span class="dsp-curve-note">Значение нового ключа; изменение добавляет ключ на курсоре</span>
        {/if}
      {/each}
      {#if effect.id.startsWith('filmcraft:')}<AudioProjectResponse {effect} {localTick} />{/if}
    </div>
  {/each}
  {#if effects.some(effect => !effect.bypass)}<p class="composition-help">DSP применяется в нативном экспорте; браузерное воспроизведение пока без DSP.</p>{/if}
</div>

<style>
  .dsp-curve-note { display: block; margin-block: .25rem; }
  .dsp-keys { max-height: 16rem; overflow: auto; }
  .dsp-key { display: flex; flex-wrap: wrap; align-items: end; gap: .5rem; }
  .dsp-key label { display: grid; }
  .dsp-key input { width: 7rem; }
</style>
