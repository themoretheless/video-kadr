<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Checkerboard = Extract<WireVideoEffect, { kind: 'checkerboard' }>
  type Channel = 'anchorX'|'anchorY'|'width'|'height'|'feather'|'opacity'|'colorR'|'colorG'|'colorB'|'colorA'
  const defaults={width:64,height:64,feather:0,opacity:100} as const
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['anchorX','Якорь X, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['anchorY','Якорь Y, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['width','Ширина, px',0,4000,64],['height','Высота, px',0,4000,64],['feather','Растушёвка, px',0,100,0],['opacity','Непрозрачность, %',0,100,100],['colorR','Красный',-3.4028234663852886e38,3.4028234663852886e38,1],['colorG','Зелёный',-3.4028234663852886e38,3.4028234663852886e38,1],['colorB','Синий',-3.4028234663852886e38,3.4028234663852886e38,1],['colorA','Альфа цвета',-3.4028234663852886e38,3.4028234663852886e38,1]]
  function sourceAnchor(_sourceId:string):[number,number] {return [0,0]}
  function value(effect: Checkerboard, channel: Channel, base: number, _sourceId: string): number | undefined {
    if(channel==='width'||channel==='height'||channel==='feather'||channel==='opacity')return effect.parameters[channel]??base
    if(channel==='anchorX'||channel==='anchorY')return effect.parameters.anchor?.[channel==='anchorX'?0:1]??undefined
    return effect.parameters.color?.[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]??base
  }
  function setValue(effect: Checkerboard, channel: Channel, number: number | null, _sourceId: string): Checkerboard {
    if(channel==='width'||channel==='height'||channel==='feather'||channel==='opacity')return {...effect,parameters:{...effect.parameters,[channel]:number??defaults[channel]}}
    if(channel==='anchorX'||channel==='anchorY'){
      const anchor:[number|null,number|null]=[...(effect.parameters.anchor??[null,null])]
      anchor[channel==='anchorX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,anchor}}
    }
    const color:[number,number,number,number]=[...(effect.parameters.color??[1,1,1,1])]
    color[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]=number??[1,1,1,1][{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]!
    return {...effect,parameters:{...effect.parameters,color}}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: Checkerboard) => Checkerboard): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'checkerboard' ? change(effect) : effect))
  }
  function keyframe(effect: Checkerboard, channel: Channel, base: number, start: number, sourceId: string): Checkerboard {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceAnchor(sourceId)[channel === 'anchorX' ? 0 : 1] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: Checkerboard, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): Checkerboard {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Checkerboard">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'checkerboard', parameters: {} }])}>Добавить Checkerboard</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'checkerboard'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Checkerboard {index + 1}</legend>
              <label>Режим<select aria-label={`${clip.id} Checkerboard mode`} value={effect.parameters.mode??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,mode:Number(event.currentTarget.value)}}))}><option value="0">None</option><option value="1">Normal</option><option value="2">Add</option><option value="3">Multiply</option><option value="4">Screen</option><option value="5">Overlay</option></select></label>
              <label><input aria-label={`${clip.id} Checkerboard square`} type="checkbox" checked={effect.parameters.square??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,square:event.currentTarget.checked}}))} />Square (Width Only)</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Checkerboard ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Checkerboard ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Checkerboard ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Checkerboard ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Checkerboard ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Checkerboard ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, anchor: [null,null] } }))}>Автоматический якорь Checkerboard</button>
              <small>Пустая координата якоря — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'checkerboard', parameters: {} }))}>Сбросить Checkerboard</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Checkerboard</button></div>
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
  .row { display: flex; gap: 6px; align-items: anchor; flex-wrap: wrap; }
  label { display: flex; gap: 6px; align-items: anchor; }
  input[type=number] { width: 5.5rem; }
</style>
