<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type PaintBucket = Extract<WireVideoEffect, { kind: 'paint_bucket' }>
  type Channel = 'pointX'|'pointY'|'tolerance'|'colorR'|'colorG'|'colorB'|'colorA'|'opacity'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['pointX','Точка X, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['pointY','Точка Y, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['tolerance','Допуск',0,255,10],['colorR','Красный',-3.4028234663852886e38,3.4028234663852886e38,1],['colorG','Зелёный',-3.4028234663852886e38,3.4028234663852886e38,0],['colorB','Синий',-3.4028234663852886e38,3.4028234663852886e38,0],['colorA','Альфа цвета',-3.4028234663852886e38,3.4028234663852886e38,1],['opacity','Непрозрачность, %',0,100,100]]
  function sourcePoint(sourceId: string): [number,number] {
    const source = nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if (!source) throw new Error('Не найдены размеры исходного кадра')
    return [source.width / 2, source.height / 2]
  }
  function value(effect: PaintBucket, channel: Channel, base: number, _sourceId: string): number | undefined {
    if(channel==='tolerance'||channel==='opacity')return effect.parameters[channel]??base
    if(channel==='pointX'||channel==='pointY')return effect.parameters.point?.[channel==='pointX'?0:1]??undefined
    return effect.parameters.color?.[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]??base
  }
  function setValue(effect: PaintBucket, channel: Channel, number: number | null, _sourceId: string): PaintBucket {
    if(channel==='tolerance'||channel==='opacity')return {...effect,parameters:{...effect.parameters,[channel]:number??(channel==='tolerance'?10:100)}}
    if(channel==='pointX'||channel==='pointY'){
      const center:[number|null,number|null]=[...(effect.parameters.point??[null,null])]
      center[channel==='pointX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,point:center}}
    }
    const color:[number,number,number,number]=[...(effect.parameters.color??[1,0,0,1])]
    color[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]=number??[1,0,0,1][{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]!
    return {...effect,parameters:{...effect.parameters,color}}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: PaintBucket) => PaintBucket): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'paint_bucket' ? change(effect) : effect))
  }
  function keyframe(effect: PaintBucket, channel: Channel, base: number, start: number, sourceId: string): PaintBucket {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourcePoint(sourceId)[channel === 'pointX' ? 0 : 1] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: PaintBucket, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): PaintBucket {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="PaintBucket">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'paint_bucket', parameters: {} }])}>Добавить PaintBucket</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'paint_bucket'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>PaintBucket {index + 1}</legend>
              <label>Выбор области<select aria-label={`${clip.id} PaintBucket selector`} value={effect.parameters.selector??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,selector:Number(event.currentTarget.value)}}))}>{#each ['Color & Alpha', 'Straight Color', 'Transparency', 'Opacity'] as name,choice}<option value={choice}>{name}</option>{/each}</select></label>
              <label>Наложение<select aria-label={`${clip.id} PaintBucket mode`} value={effect.parameters.mode??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,mode:Number(event.currentTarget.value)}}))}>{#each ['Normal', 'Behind', 'Add', 'Multiply', 'Screen'] as name,choice}<option value={choice}>{name}</option>{/each}</select></label>
              <label><input aria-label={`${clip.id} PaintBucket invert`} type="checkbox" checked={effect.parameters.invert??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,invert:event.currentTarget.checked}}))} />Invert Fill</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} PaintBucket ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} PaintBucket ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} PaintBucket ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} PaintBucket ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} PaintBucket ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} PaintBucket ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, point: [null,null] } }))}>Автоматическая точка PaintBucket</button>
              <small>Пустая координата точки — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'paint_bucket', parameters: {} }))}>Сбросить PaintBucket</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить PaintBucket</button></div>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  section, fieldset { display: grid; gap: 6px; }
  fieldset { border: 0; padding: 0; margin: 0; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  label { display: flex; gap: 6px; align-items: center; }
  input[type=number] { width: 5.5rem; }
</style>
