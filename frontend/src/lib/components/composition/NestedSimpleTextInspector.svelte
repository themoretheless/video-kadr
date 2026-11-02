<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type SimpleText = Extract<WireVideoEffect, { kind: 'simple_text' }>
  type Channel='positionX'|'positionY'|'size'|'colorR'|'colorG'|'colorB'|'colorA'|'bgOpacity'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[['positionX','Позиция X',undefined,undefined,0],['positionY','Позиция Y',undefined,undefined,0],['size','Размер',1,1000,64],['colorR','Цвет R',0,1,1],['colorG','Цвет G',0,1,1],['colorB','Цвет B',0,1,1],['colorA','Цвет A',0,1,1],['bgOpacity','Фон, %',0,100,0]]
  function value(effect:SimpleText,channel:Channel,base:number):number{
    if(channel==='positionX'||channel==='positionY'){
      const axis=channel==='positionX'?0:1
      return effect.parameters.position?.[axis]??base
    }
    if(channel.startsWith('color'))return effect.parameters.color?.[['colorR','colorG','colorB','colorA'].indexOf(channel)]??1
    return effect.parameters[channel as 'size'|'bgOpacity']??base
  }
  function displayed(effect:SimpleText,channel:Channel,base:number):number|undefined{
    if(channel==='positionX'||channel==='positionY')return effect.parameters.position?.[channel==='positionX'?0:1]??undefined
    return value(effect,channel,base)
  }
  function setValue(effect:SimpleText,channel:Channel,number:number|null):SimpleText{
    if(channel==='positionX'||channel==='positionY'){
      const position:[number|null,number|null]=[...(effect.parameters.position??[null,null])]
      position[channel==='positionX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,position}}
    }
    if(channel.startsWith('color')){
      const color:[number,number,number,number]=[...(effect.parameters.color??[1,1,1,1])]
      color[['colorR','colorG','colorB','colorA'].indexOf(channel)]=number??1
      return {...effect,parameters:{...effect.parameters,color}}
    }
    return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}
  }
  const choices=[['font','Шрифт',0,['Inter','JetBrains Mono']],['alignment','Выравнивание',1,['Слева','По центру','Справа']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: SimpleText) => SimpleText): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'simple_text' ? change(effect) : effect))
  }
  function keyframe(effect: SimpleText, channel: Channel, base: number, start: number): SimpleText {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: SimpleText, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): SimpleText {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Simple Text">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'simple_text', parameters: {} }])}>Добавить Simple Text</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'simple_text'}
            {@const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[clip.sourceId]}
            {@const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Simple Text {index + 1}</legend>
              <label>Текст<textarea aria-label={`${clip.id} Simple Text text`} value={effect.parameters.text??'Simple Text'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,text:event.currentTarget.value}}))}></textarea></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Simple Text ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={displayed(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Simple Text ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, channel==='positionX'?(source?.width??canvas?.width??0)*0.5:channel==='positionY'?(source?.height??canvas?.height??0)*0.5:base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Simple Text ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Simple Text ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Simple Text ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Simple Text ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} Simple Text ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'simple_text', parameters: {} }))}>Сбросить Simple Text</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Simple Text</button></div>
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
