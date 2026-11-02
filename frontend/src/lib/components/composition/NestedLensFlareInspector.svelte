<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type LensFlare = Extract<WireVideoEffect, { kind: 'lens_flare' }>
  type Channel = 'centerX'|'centerY'|'brightness'|'blend'|'size'|'ghosts'|'colorR'|'colorG'|'colorB'|'colorA'
  const defaults={brightness:100,blend:0,size:100,ghosts:3} as const
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['centerX','Центр X, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['centerY','Центр Y, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['brightness','Яркость, %',0,300,100],['blend','Blend With Original, %',0,100,0],['size','Размер, %',10,400,100],['ghosts','Блики-призраки',0,8,3],['colorR','Красный',-3.4028234663852886e38,3.4028234663852886e38,1],['colorG','Зелёный',-3.4028234663852886e38,3.4028234663852886e38,0.9],['colorB','Синий',-3.4028234663852886e38,3.4028234663852886e38,0.75],['colorA','Альфа цвета',-3.4028234663852886e38,3.4028234663852886e38,1]]
  function sourceCenter(sourceId: string): [number,number] {
    const source = nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if (!source) throw new Error('Не найдены размеры исходного кадра')
    return [source.width / 2, source.height / 2]
  }
  function value(effect: LensFlare, channel: Channel, base: number, _sourceId: string): number | undefined {
    if(channel==='brightness'||channel==='blend'||channel==='size'||channel==='ghosts')return effect.parameters[channel]??base
    if(channel==='centerX'||channel==='centerY')return effect.parameters.center?.[channel==='centerX'?0:1]??undefined
    return effect.parameters.color?.[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]??base
  }
  function setValue(effect: LensFlare, channel: Channel, number: number | null, _sourceId: string): LensFlare {
    if(channel==='brightness'||channel==='blend'||channel==='size'||channel==='ghosts')return {...effect,parameters:{...effect.parameters,[channel]:number??defaults[channel]}}
    if(channel==='centerX'||channel==='centerY'){
      const center:[number|null,number|null]=[...(effect.parameters.center??[null,null])]
      center[channel==='centerX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,center}}
    }
    const color:[number,number,number,number]=[...(effect.parameters.color??[1,0.9,0.75,1])]
    color[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]=number??[1,0.9,0.75,1][{colorR:0,colorG:1,colorB:2,colorA:3}[channel]]!
    return {...effect,parameters:{...effect.parameters,color}}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: LensFlare) => LensFlare): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'lens_flare' ? change(effect) : effect))
  }
  function keyframe(effect: LensFlare, channel: Channel, base: number, start: number, sourceId: string): LensFlare {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceCenter(sourceId)[channel === 'centerX' ? 0 : 1] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: LensFlare, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): LensFlare {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="LensFlare">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'lens_flare', parameters: {} }])}>Добавить Lens Flare</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'lens_flare'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Lens Flare {index + 1}</legend>
              <label>Объектив<select aria-label={`${clip.id} LensFlare lensType`} value={effect.parameters.lensType??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,lensType:Number(event.currentTarget.value)}}))}><option value="0">50–300mm Zoom</option><option value="1">35mm Prime</option><option value="2">105mm Prime</option><option value="3">Anamorphic</option></select></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} LensFlare ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} LensFlare ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} LensFlare ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} LensFlare ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} LensFlare ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} LensFlare ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, center: [null,null] } }))}>Автоматический центр Lens Flare</button>
              <small>Пустая координата центра — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'lens_flare', parameters: {} }))}>Сбросить Lens Flare</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Lens Flare</button></div>
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
