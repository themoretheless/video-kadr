<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireCreativeGrade, WireColorWheel, WireCreativeAnimation } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  const looks = ['Нет', 'Teal & Orange', 'Warm Film', 'Cool Blue', 'Bleach Bypass', 'Faded Matte', 'Monochrome', 'Golden Hour', 'Night']
  const wheels = ['shadows', 'midtones', 'highlights'] as const
  const labels = { shadows: 'Тени', midtones: 'Средние тона', highlights: 'Света' }
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function update(track: string, clip: string, index: number, change: (grade: WireCreativeGrade) => WireCreativeGrade): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'creative_grade' ? {...effect, parameters: change(effect.parameters)} : effect))
  }
  function axis(grade: WireCreativeGrade, wheel: typeof wheels[number], key: keyof WireColorWheel, value: number): WireCreativeGrade {
    return {...grade, [wheel]: {...grade[wheel], [key]: value}}
  }
  type Channel = keyof WireCreativeAnimation
  function channel(wheel: typeof wheels[number], key: string): Channel {
    return `${wheel}${key[0]!.toUpperCase()}${key.slice(1)}` as Channel
  }
  function animate(track: string, clip: string, index: number, name: Channel, start: number, remove = false): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => {
      if(i!==index || effect.kind!=='creative_grade') return effect
      const animation={...effect.animation}
      if(remove) {delete animation[name];return {...effect,animation}}
      const tick=nestedComposition.timeUs-start
      if(!Number.isSafeInteger(tick) || tick<0) throw new Error('Курсор должен находиться после начала клипа')
      const previous=animation[name]
      if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000) throw new Error('Шкала канала должна быть в микросекундах')
      let value=effect.parameters.intensity ?? 100
      if(name!=='intensity') {
        const wheel=wheels.find(w=>name.startsWith(w))!
        const suffix=name.slice(wheel.length)
        const key=(suffix[0]!.toLowerCase()+suffix.slice(1)) as keyof WireColorWheel
        value=effect.parameters[wheel]?.[key] ?? 0
      }
      const oldPoints=previous?.mode==='keyframes'?previous.track.keyframes:[]
      const points=[...oldPoints.filter(p=>p.tick!==tick),{...oldPoints.find(p=>p.tick===tick),tick,value}].sort((a,b)=>a.tick-b.tick)
      animation[name]={mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes:points}:{timeBase:1000000,interpolation:'linear',keyframes:points}}
      return {...effect,animation}
    }))
  }
  function reset(track: string, clip: string, index: number): void {
    void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='creative_grade'?{kind:'creative_grade',parameters:{}}:effect))
  }
</script>
<section aria-label="Цветовые колёса и образы">
 {#each tracks as track (track.id)}
  {#if track.kind === 'video' || track.kind === 'image'}
   {#each track.clips as clip (clip.id)}
    <div class="clip-row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, {kind:'creative_grade',parameters:{}}])}>Добавить колёса и образ</button></div>
    {#each (clip.effects ?? []) as effect, index}
     {#if effect.kind === 'creative_grade'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Колёса и образ {index + 1}</legend>
       <div class="clip-row">
        <label>Образ <select aria-label={`${clip.id} образ`} value={effect.parameters.look ?? 0} onchange={e => {const value=Number(e.currentTarget.value);update(track.id,clip.id,index,grade=>({...grade,look:value}))}}>{#each looks as look, i}<option value={i}>{look}</option>{/each}</select></label>
        <label>Интенсивность, % <input aria-label={`${clip.id} интенсивность образа`} type="number" min="0" max="200" step="1" value={effect.parameters.intensity ?? 100} onchange={e => {const value=Number(e.currentTarget.value);update(track.id,clip.id,index,grade=>({...grade,intensity:value}))}} /></label>
        <button class="btn ghost sm" aria-label={`${clip.id} intensity ключ`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>animate(track.id,clip.id,index,'intensity',('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick))}>Ключ интенсивности</button>
        {#if effect.animation?.intensity}<button class="btn ghost sm" aria-label={`${clip.id} intensity удалить анимацию`} onclick={()=>animate(track.id,clip.id,index,'intensity',('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick),true)}>Удалить анимацию интенсивности</button>{/if}
       </div>
       {#if effect.animation?.intensity?.mode==='keyframes'}<small>{effect.animation.intensity.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
       <div class="wheels">
        {#each wheels as wheel}
         <div class="wheel">
          <span>{labels[wheel]}</span>
          <button class="wheel-control" aria-label={`Выбрать цвет ${labels[wheel]}`} onclick={event => {
            const rect=event.currentTarget.getBoundingClientRect()
            const x=Math.max(-1,Math.min(1,(event.clientX-rect.left-rect.width/2)/(rect.width*0.46)))
            const y=Math.max(-1,Math.min(1,(rect.top+rect.height/2-event.clientY)/(rect.height*0.46)))
            update(track.id,clip.id,index,grade=>({...grade,[wheel]:{...grade[wheel],x:event.detail===0?0:x,y:event.detail===0?0:y}}))
          }}>
          <svg viewBox="0 0 100 100" role="img" aria-label={`Колесо ${labels[wheel]}`}>
           <circle cx="50" cy="50" r="46" class="ring" />
           <path d="M4 50H96M50 4V96" class="axes" />
           <circle cx={50+(effect.parameters[wheel]?.x ?? 0)*46} cy={50-(effect.parameters[wheel]?.y ?? 0)*46} r="3" class="position" />
          </svg>
          </button>
          {#each ['x','y','lightness'] as key}
           {@const currentAnimation=effect.animation?.[channel(wheel,key)]}
           <div class="channel-row"><label>{key === 'lightness' ? 'Яркость, %' : key.toUpperCase()} <input aria-label={`${clip.id} ${labels[wheel]} ${key}`} type="number" min={key==='lightness'?-100:-1} max={key==='lightness'?100:1} step={key==='lightness'?1:0.01} value={effect.parameters[wheel]?.[key as keyof WireColorWheel] ?? 0} onchange={e => {const value=Number(e.currentTarget.value);update(track.id,clip.id,index,grade=>axis(grade,wheel,key as keyof WireColorWheel,value))}} /></label>
           <button class="btn ghost sm" aria-label={`${clip.id} ${channel(wheel,key)} ключ`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>animate(track.id,clip.id,index,channel(wheel,key),('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick))}>Ключ</button>
           {#if effect.animation?.[channel(wheel,key)]}<button class="btn ghost sm" aria-label={`${clip.id} ${channel(wheel,key)} удалить анимацию`} onclick={()=>animate(track.id,clip.id,index,channel(wheel,key),('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick),true)}>Удалить анимацию</button>{/if}
           </div>
           {#if currentAnimation?.mode==='keyframes'}<small>{currentAnimation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
          {/each}
         </div>
        {/each}
       </div>
       <button class="btn ghost sm" onclick={() => reset(track.id,clip.id,index)}>Сбросить колёса и образ</button>
       <button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить колёса и образ</button>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>
 .channel-row {display:flex;gap:0.25rem;align-items:center;flex-wrap:wrap;}
 small {display:block;}
 .clip-row, .wheels {display:flex;gap:0.75rem;align-items:center;flex-wrap:wrap;}
 fieldset {border:0;padding:0.5rem 0;margin:0;}
 .wheel {display:flex;flex-direction:column;gap:0.25rem;}
 .wheel-control {width:100px;height:100px;padding:0;border:0;border-radius:50%;background:radial-gradient(circle,#eee 0%,transparent 68%),conic-gradient(from 90deg,#f55,#66f,#5d5,#f55);cursor:crosshair;}
 svg {width:100px;height:100px;display:block;} .ring,.axes {fill:none;stroke:var(--border,#555);} .position {fill:#a8c8ff;}
 input {width:4.5rem;}
</style>
