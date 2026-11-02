<script lang="ts">
  import {sampleAnimatableValue} from '$lib/composition/keyframes'
  import {inspectInsta360Source,type Insta360SourceMetadata} from "$lib/api"
  import Insta360ProfileImport from "./Insta360ProfileImport.svelte"
  import SphericalSemanticTracker from "./SphericalSemanticTracker.svelte"
  import Insta360GyroImport from "./Insta360GyroImport.svelte"
  import SphericalSubjectTracker from "./SphericalSubjectTracker.svelte"
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import {parseOrientationSidecar,parseLensCalibration,defaultLenses} from '$lib/motion/sphericalVideo'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false, media=[], token="" }: { disabled?: boolean;media?:readonly {id:string;url:string}[];token?:string } = $props()
  type SphericalVideo = Extract<WireVideoEffect, { kind: 'spherical_video' }>
  type Channel='tilt'|'pan'|'roll'|'fov'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[["tilt", "Наклон X, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 0], ["pan", "Поворот Y, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 0], ["roll", "Вращение Z, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 0], ["fov", "Поле зрения, °", 10, 179, 90]]
  function value(effect:SphericalVideo,channel:Channel,base:number):number{return effect.parameters[channel]??base}
  function setValue(effect:SphericalVideo,channel:Channel,number:number|null):SphericalVideo{return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}}
  let error = $state('')
  let cameraInfo=$state<Record<string,Insta360SourceMetadata|null>>({}),readingSource=$state('')
  let cameraController:AbortController|undefined
  $effect(()=>()=>cameraController?.abort())
  async function readCamera(id:string){if(readingSource)return;const active=new AbortController();cameraController=active;readingSource=id;error='';try{cameraInfo[id]=await inspectInsta360Source(id,token,active.signal)}catch(cause){if(!active.signal.aborted)error=cause instanceof Error?cause.message:String(cause)}finally{if(cameraController===active){cameraController=undefined;readingSource=''}}}

  async function importOrientation(file:File|undefined,track:string,clip:string,index:number):Promise<void>{
    if(!file)return
    try {if(file.size>4*1024*1024)throw new Error('Файл ориентации должен быть меньше 4 МБ');const orientation=parseOrientationSidecar(JSON.parse(await file.text()));edit(track,clip,index,current=>({...current,parameters:{...current.parameters,orientation}}));error=''}catch(cause){error=cause instanceof Error?cause.message:String(cause)}
  }
  function calibration(text:string,track:string,clip:string,index:number):void{
    try {const lenses=parseLensCalibration(JSON.parse(text));edit(track,clip,index,current=>({...current,parameters:{...current.parameters,lenses,cameraProfile:undefined}}));error=''}catch(cause){error=cause instanceof Error?cause.message:String(cause)}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: SphericalVideo) => SphericalVideo): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'spherical_video' ? change(effect) : effect))
  }
  function keyframe(effect: SphericalVideo, channel: Channel, base: number, start: number): SphericalVideo {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: SphericalVideo, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): SphericalVideo {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="360° видео">
  {#if error}<p role="alert">{error}</p>{/if}
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'spherical_video', parameters: {} }])}>Добавить 360° видео</button></div>
        {#each (clip.effects ?? []) as effect, index (index)}
          {#if effect.kind === 'spherical_video'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>360° видео {index + 1}</legend>
              {#each channels as [channel, label, min, max, base] (channel)}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} 360° видео ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} 360° видео ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} 360° видео ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} 360° видео ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} 360° видео ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} 360° видео ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <label>Исходник<select aria-label={`${clip.id} spherical input`} value={effect.parameters.input??'equirectangular'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,input:event.currentTarget.value as 'equirectangular'|'dual_fisheye',sourceLayout:event.currentTarget.value==='equirectangular'?'packed':current.parameters.sourceLayout,cameraProfile:event.currentTarget.value==='equirectangular'?undefined:current.parameters.cameraProfile,rollingShutter:event.currentTarget.value==='equirectangular'?undefined:current.parameters.rollingShutter,stitching:event.currentTarget.value==='equirectangular'?undefined:current.parameters.stitching}}))}><option value="equirectangular">Готовая панорама 360°</option><option value="dual_fisheye">Два fisheye в одном кадре</option></select></label>
              <label>Проекция<select aria-label={`${clip.id} spherical output`} value={effect.parameters.output??'perspective'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,output:event.currentTarget.value as 'perspective'|'equirectangular'|'tiny_planet'}}))}><option value="perspective">Перспективный кадр</option><option value="equirectangular">Панорама 360°</option><option value="tiny_planet">Маленькая планета</option></select></label>
              {#if effect.parameters.input==='dual_fisheye'}
                <label>Сшивка<select aria-label={`${clip.id} spherical stitching`} value={effect.parameters.stitching?.mode??'feather'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,stitching:{...current.parameters.stitching,mode:event.currentTarget.value as 'feather'|'optical_flow'}}}))}><option value="feather">Калибровка и растушёвка</option><option value="optical_flow">Optical flow: совмещение шва</option></select></label>
                {#if effect.parameters.stitching?.mode==='optical_flow'}<label>Сила коррекции<input aria-label={`${clip.id} optical flow strength`} type="number" min="0" max="1" step="0.05" value={effect.parameters.stitching.strength??1} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,stitching:{...current.parameters.stitching,strength:Number(event.currentTarget.value)}}}))}/></label>{/if}
                {#if effect.parameters.cameraProfile}<p>Камерная калибровка: {effect.parameters.cameraProfile.model}</p><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,current=>{const parameters={...current.parameters};delete parameters.cameraProfile;return {...current,parameters}})}>Убрать профиль камеры</button>{/if}
                <label>Объективы<select aria-label={`${clip.id} spherical source layout`} value={effect.parameters.sourceLayout??'packed'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,sourceLayout:event.currentTarget.value as 'packed'|'dual_video_streams'}}))}><option value="packed">В одном видеопотоке</option><option value="dual_video_streams">Два видеопотока одного файла</option></select></label>
                <label>Калибровка объективов, JSON<textarea aria-label={`${clip.id} spherical lenses`} value={JSON.stringify(effect.parameters.lenses??defaultLenses)} onchange={event=>calibration(event.currentTarget.value,track.id,clip.id,index)}></textarea></label>
                <p>Укажите параметры объективов своей камеры. Начальные значения — пример расположения, не профиль Insta360.</p>
              {/if}
              {#if track.kind==='video'&&'placement' in clip&&index===0&&(effect.parameters.input??'equirectangular')==='equirectangular'&&!effect.parameters.orientation?.length&&clip.placement.speed===1&&!clip.placement.speedRamp&&!clip.placement.concatRetime&&!clip.effectTiming&&!(track.transitions??[]).some(t=>t.concatOverlap&&(t.fromClipId===clip.id||t.toClipId===clip.id))&&(clip.playbackMode?.mode??'forward')==='forward'&&(clip.stabilization?.mode??'disabled')==='disabled'}
                {@const source=media.find(entry=>entry.id===clip.sourceId)}
                {#if source}<SphericalSubjectTracker url={source.url} binding={clip} sourceInSeconds={clip.placement.sourceInTick/1000000} duration={(clip.placement.sourceOutTick-clip.placement.sourceInTick)/1000000} disabled={disabled||nestedHistory.busy||track.locked} onapply={animation=>edit(track.id,clip.id,index,current=>({...current,animation:{...current.animation,...animation}}))}/>{/if}
              {/if}
              {#if track.kind==='video'&&'placement' in clip&&token&&nestedComposition.projectId&&nestedComposition.revision!==null&&index===0&&clip.placement.speed===1&&!clip.placement.speedRamp&&!clip.placement.concatRetime&&!clip.effectTiming&&!(track.transitions??[]).some(t=>t.concatOverlap&&(t.fromClipId===clip.id||t.toClipId===clip.id))&&(clip.playbackMode?.mode??'forward')==='forward'&&(clip.stabilization?.mode??'disabled')==='disabled'}
                {#each nestedComposition.document?.compositions[compositionId]?.content.metadata?.mediaVersions?.[clip.id]?.videoCutout?.trackedRegions??[] as region (region.maskSourceId)}
                  <SphericalSemanticTracker {region} {token} spherical={effect.parameters} allowFraming={(effect.parameters.output??'perspective')==='perspective'} initialFov={effect.animation?.fov?sampleAnimatableValue(effect.animation.fov,0):effect.parameters.fov??90} canvasAspect={(nestedComposition.document?.compositions[compositionId]?.content.canvas.width??2)/(nestedComposition.document?.compositions[compositionId]?.content.canvas.height??1)} projectId={nestedComposition.projectId} revision={nestedComposition.revision} {compositionId} clipId={clip.id} sourceInSeconds={clip.placement.sourceInTick/1000000} duration={(clip.placement.sourceOutTick-clip.placement.sourceInTick)/1000000} binding={clip} disabled={disabled||nestedHistory.busy||track.locked} onapply={animation=>edit(track.id,clip.id,index,current=>({...current,animation:{...current.animation,...animation}}))}/>
                {/each}
              {/if}
              {#if track.kind==='video'&&'placement' in clip&&token&&index===0&&clip.placement.speed===1&&!clip.placement.speedRamp&&!clip.placement.concatRetime&&!clip.effectTiming&&!(track.transitions??[]).some(t=>t.concatOverlap&&(t.fromClipId===clip.id||t.toClipId===clip.id))&&(clip.playbackMode?.mode??'forward')==='forward'}
                <Insta360GyroImport sourceId={clip.sourceId} {token} startSeconds={clip.placement.sourceInTick/1000000} durationSeconds={(clip.placement.sourceOutTick-clip.placement.sourceInTick)/1000000} allowRollingShutter={effect.parameters.input==='dual_fisheye'} binding={clip} disabled={disabled||nestedHistory.busy||track.locked} onapply={parameters=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,...parameters}}))}/>
              {/if}
              {#if 'sourceId' in clip&&token}
                <Insta360ProfileImport sourceId={clip.sourceId} {token} binding={clip} disabled={disabled||nestedHistory.busy||track.locked} onapply={analysis=>{if(analysis.calibration.profile&&analysis.sourceLayout)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,input:'dual_fisheye',sourceLayout:analysis.sourceLayout!,cameraProfile:analysis.calibration.profile!}}))}}/>
                <button class="btn ghost sm" disabled={Boolean(readingSource)} onclick={()=>void readCamera(clip.sourceId)}>Сведения камеры</button>
                {#if clip.sourceId in cameraInfo}
                  {@const info=cameraInfo[clip.sourceId]}
                  {#if info}<p>{info.cameraModel??'Insta360'} · {info.firmware??'версия не указана'}{info.frameRate?` · ${info.frameRate} fps`:''}</p><p>Полей калибровки: {info.calibrations.length}. Автоматический профиль доступен для проверенной калибровки и пары видеопотоков.</p>{:else}<p>Метаданные Insta360 v3 не найдены в этом исходнике.</p>{/if}
                {/if}
              {/if}
              <label>Стабилизация<select aria-label={`${clip.id} spherical stabilization`} value={effect.parameters.stabilizationMode??'locked'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,stabilizationMode:event.currentTarget.value as 'disabled'|'locked'|'smooth'|'horizon'}}))}><option value="disabled">Без фиксации направления</option><option value="locked">Зафиксировать направление</option><option value="smooth">Плавное движение камеры</option><option value="horizon">Удерживать горизонт</option></select></label>
              {#if effect.parameters.stabilizationMode==='smooth'}<label>Сглаживание, с<input aria-label={`${clip.id} spherical smoothing`} type="number" min="0.01" max="10" step="0.05" value={effect.parameters.smoothSeconds??.5} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,smoothSeconds:Number(event.currentTarget.value)}}))}/></label>{/if}
              <label>Ориентация камеры, JSON<input aria-label={`${clip.id} spherical orientation`} type="file" accept=".json,application/json" onchange={event=>void importOrientation(event.currentTarget.files?.[0],track.id,clip.id,index)}/></label>
              <p>Кватернионы camera-to-world [w,x,y,z] или gyro в рад/с, оси X вправо, Y вверх, Z вперёд. Время относительно начала клипа. Для raw gyro требуется калибровка осей и смещения.</p>
              {#if effect.parameters.orientation?.length}
                <label>Сдвиг ориентации, с<input aria-label={`${clip.id} spherical orientation offset`} type="number" step="0.001" value={effect.parameters.orientationOffsetSeconds??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,orientationOffsetSeconds:Number(event.currentTarget.value)}}))}/></label>
                <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,orientation:[]}}))}>Убрать ориентацию</button>
              {/if}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'spherical_video', parameters: {} }))}>Сбросить 360° видео</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить 360° видео</button></div>
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
