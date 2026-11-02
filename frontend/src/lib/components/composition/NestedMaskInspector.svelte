<script lang="ts">
 import CurveParameters from './CurveParameters.svelte'
 import {nestedComposition,nestedHistory,editNestedVideoEffects} from '$lib/state/nestedComposition.svelte'
 import {normalizeKeyframeTangents,resolvedKeyframeTangents} from '$lib/composition/tangents'
 import {sampleAnimatableValue} from '$lib/composition/keyframes'
 import type {WireVideoEffect,WireAnimatableValue,CompositionSimpleInterpolation,CompositionInterpolation,CompositionExtrapolationMode,CompositionKeyframeTangents} from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Mask=Extract<WireVideoEffect,{kind:'mask'|'linear_mask'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 const constant=(value:number):WireAnimatableValue=>({mode:'constant',value})
 function add(track:string,clip:string,shape:'rectangle'|'ellipse'|'linear') {
  const common={x:constant(0.5),y:constant(0.5),rotationDegrees:constant(0),feather:0,inverted:false}
  const effect:Mask=shape==='linear'?{kind:'linear_mask',...common}:{kind:'mask',shape,...common,width:constant(1),height:constant(1)}
  void editNestedVideoEffects(compositionId,track,clip,effects=>[...effects,effect])
 }
 function edit(track:string,clip:string,index:number,change:(effect:Mask)=>Mask) {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index&&(effect.kind==='mask'||effect.kind==='linear_mask')?change(effect):effect))
 }
 const channels=['x','y','rotationDegrees','width','height'] as const
 const labels={x:'X',y:'Y',rotationDegrees:'Поворот',width:'Ширина',height:'Высота'}
 const interpolations:readonly (readonly [CompositionSimpleInterpolation,string])[]=[['hold','Ступенчатая'],['linear','Линейная'],['ease_in','Ускорение'],['ease_out','Замедление'],['ease_in_out','Плавная'],['ease_in_out_cubic','Плавная (совместимая)'],['bounce_in','Отскок в начале'],['bounce_out','Отскок в конце'],['bounce_in_out','Отскок с двух сторон']]
 const extrapolations:readonly (readonly [CompositionExtrapolationMode,string])[]=[['hold','Удерживать'],['continue','Продолжать'],['loop','Повторять'],['ping_pong','Отражать'],['offset','Повторять со сдвигом']]
 function value(animation:WireAnimatableValue,local=0):number {return sampleAnimatableValue(animation,Math.max(0,local),1000000)}
 function key(effect:Mask,channel:typeof channels[number],start:number):Mask {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
  if((channel==='width'||channel==='height')&&effect.kind!=='mask')return effect
  const animation=channel==='width'||channel==='height'?(effect as Extract<Mask,{kind:'mask'}>)[channel]:effect[channel]
  if(animation.mode==='keyframes'&&animation.track.timeBase!==1000000)throw new Error('Шкала маски должна быть в микросекундах')
  const previous=animation.mode==='keyframes'?animation.track.keyframes:[]
  const keyframes=[...previous.filter(point=>point.tick!==tick),{...previous.find(point=>point.tick===tick),tick,value:value(animation,tick)}].sort((a,b)=>a.tick-b.tick)
  return {...effect,[channel]:{mode:'keyframes',track:animation.mode==='keyframes'?{...animation.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }

 function animationFor(effect:Mask,channel:typeof channels[number]):WireAnimatableValue|undefined {
  if(channel==='width'||channel==='height')return effect.kind==='mask'?effect[channel]:undefined
  return effect[channel]
 }
 function editKey(effect:Mask,channel:typeof channels[number],tick:number,patch:{tick?:number;value?:number;interpolation?:CompositionInterpolation; tangents?:CompositionKeyframeTangents}|null):Mask {
  const animation=animationFor(effect,channel)
  if(animation?.mode!=='keyframes')return effect
  const keyframes=animation.track.keyframes.flatMap(point=>point.tick===tick?patch?[{...point,...patch}]:[]:[point]).sort((a,b)=>a.tick-b.tick)
  return {...effect,[channel]:keyframes.length?{...animation,track:{...animation.track,keyframes:normalizeKeyframeTangents(keyframes,animation.track.interpolation)}}:constant(value(animation))}
 }

 function setCurve(effect:Mask,channel:typeof channels[number],interpolation:CompositionInterpolation):Mask {
  const curve=animationFor(effect,channel)
  return curve?.mode==='keyframes'?{...effect,[channel]:{...curve,track:{...curve.track,interpolation}}}:effect
 }
 function selectCurve(token:string):CompositionInterpolation {
  if(token==='bezier')return {kind:'bezier',p1:[0.25,0.1],p2:[0.25,1]}
  if(token==='elastic')return {kind:'elastic',dir:'out',amplitude:1,period:0.3}
  if(interpolations.some(([value])=>value===token))return token as CompositionSimpleInterpolation
  throw new Error('Неизвестная интерполяция')
 }

</script>
<section aria-label="Маски слоёв">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    {@const local=nestedComposition.timeUs-('placement' in clip?clip.placement.timelineStartTick:clip.timelineStartTick)}
    <div class="row"><span>{track.name} · {clip.id}</span>
     {#each [['rectangle','Прямоугольник'],['ellipse','Эллипс'],['linear','Линейная']] as [shape,label]}
      <button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked} onclick={()=>add(track.id,clip.id,shape as 'rectangle'|'ellipse'|'linear')}>{label}</button>
     {/each}
    </div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='mask'||effect.kind==='linear_mask'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked}>
       <legend>Маска {index+1} · {effect.kind==='mask'?effect.shape:'linear'}</legend>
       <div class="row">
        {#each channels as channel}
         {#if channel!=='width'&&channel!=='height'}
          <label>{labels[channel]}<input aria-label={`${clip.id} mask ${index} ${channel}`} type="number" step="0.01" value={value(effect[channel],local)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,[channel]:constant(Number(event.currentTarget.value))}))} /></label>
         {:else if effect.kind==='mask'}
          <label>{labels[channel]}<input aria-label={`${clip.id} mask ${index} ${channel}`} type="number" min="0.001" step="0.01" value={value(effect[channel],local)} onchange={event=>edit(track.id,clip.id,index,current=>current.kind==='mask'?{...current,[channel]:constant(Number(event.currentTarget.value))}:current)} /></label>
         {/if}
         {#if (channel!=='width'&&channel!=='height')||effect.kind==='mask'}
          <button class="btn ghost sm" aria-label={`${clip.id} mask ${index} ${channel} key`} disabled={nestedComposition.timeUs<('placement' in clip?clip.placement.timelineStartTick:clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>key(current,channel,'placement' in clip?clip.placement.timelineStartTick:clip.timelineStartTick))}>Ключ {labels[channel]}</button>
         {/if}
        {/each}
        {#each channels as channel}
         {@const animation=animationFor(effect,channel)}
         {#if animation?.mode==='keyframes'}
          {@const curve=animation.track.interpolation}
          <details><summary>{labels[channel]} · {animation.track.keyframes.length} ключей</summary>
           <label>Интерполяция<select aria-label={`${clip.id} mask ${index} ${channel} interpolation`} value={typeof curve==='string'?curve:curve.kind} onchange={event=>{const interpolation=selectCurve(event.currentTarget.value);edit(track.id,clip.id,index,current=>setCurve(current,channel,interpolation))}}>
            {#each interpolations as [token,label]}<option value={token}>{label}</option>{/each}
            <option value="bezier">Bézier</option><option value="elastic">Elastic</option>
           </select></label>
           <div class="row">
            {#each ['before','after'] as side}
             <label>{side==='before'?'До первого ключа':'После последнего ключа'}<select aria-label={`${clip.id} mask ${index} ${channel} ${side}`} value={animation.track.extrapolation?.[side as 'before'|'after'] ?? 'hold'} onchange={event=>{const mode=event.currentTarget.value as CompositionExtrapolationMode;if(!extrapolations.some(([token])=>token===mode))return;edit(track.id,clip.id,index,current=>{const channelCurve=animationFor(current,channel);return channelCurve?.mode==='keyframes'?{...current,[channel]:{...channelCurve,track:{...channelCurve.track,extrapolation:{before:'hold',after:'hold',...channelCurve.track.extrapolation,[side]:mode}}}}:current})}}>
              {#each extrapolations as [token,label]}<option value={token}>{label}</option>{/each}
             </select></label>
            {/each}
           </div>
           {#if typeof curve!=='string'&&curve.kind==='bezier'}
            <div class="row">
             {#each ['p1','p2'] as control}
              {#each [0,1] as axis}
               <label>{control} {axis===0?'X':'Y'}<input type="number" step="0.01" aria-label={`${clip.id} mask ${index} ${channel} ${control} ${axis}`} value={curve[control as 'p1'|'p2'][axis]} onchange={event=>{const point:[number,number]=[...curve[control as 'p1'|'p2']];point[axis]=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>setCurve(current,channel,{...curve,[control]:point}))}} /></label>
              {/each}
             {/each}
            </div>
           {:else if typeof curve!=='string'&&curve.kind==='elastic'}
            <div class="row">
             <label>Направление<select aria-label={`${clip.id} mask ${index} ${channel} direction`} value={curve.dir} onchange={event=>{const dir=event.currentTarget.value as 'in'|'out'|'in_out';edit(track.id,clip.id,index,current=>setCurve(current,channel,{...curve,dir}))}}><option value="in">В начале</option><option value="out">В конце</option><option value="in_out">С двух сторон</option></select></label>
             {#each ['amplitude','period'] as parameter}
              <label>{parameter==='amplitude'?'Амплитуда':'Период'}<input type="number" step="0.01" aria-label={`${clip.id} mask ${index} ${channel} ${parameter}`} value={curve[parameter as 'amplitude'|'period']} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>setCurve(current,channel,{...curve,[parameter]:value}))}} /></label>
             {/each}
            </div>
           {/if}
           {#each animation.track.keyframes as point (point.tick)}
            <div class="row">
             <label>Время, с<input aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} time`} type="number" min="0" step="0.001" value={point.tick/animation.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tick:Math.round(Number(event.currentTarget.value)*animation.track.timeBase)}))} /></label>
             <label>Значение<input aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} value`} type="number" step="0.01" value={point.value} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{value:Number(event.currentTarget.value)}))} /></label>
             <label>Кривая участка<select aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} interpolation`} value={point.interpolation===undefined?'inherit':typeof point.interpolation==='string'?point.interpolation:point.interpolation.kind} onchange={event=>{const token=event.currentTarget.value;edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{interpolation:token==='inherit'?undefined:selectCurve(token)}))}}>
              <option value="inherit">Как у канала</option>
              {#each interpolations as [token,label]}<option value={token}>{label}</option>{/each}
              <option value="bezier">Bézier</option><option value="elastic">Elastic</option>
             </select></label>
             {#if typeof point.interpolation==='object'}
              <CurveParameters curve={point.interpolation} label={`${clip.id} mask ${index} ${channel} ${point.tick} curve`} onchange={interpolation=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{interpolation}))} />
             {/if}
             {#if point.tangents}
              <label>Сопряжение<select aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} continuity`} value={point.tangents.continuity} onchange={event=>{const continuity=event.currentTarget.value as 'smooth'|'broken';edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tangents:{...point.tangents!,continuity}}))}}><option value="smooth">Плавное</option><option value="broken">Независимое</option></select></label>
              {#each ['incoming','outgoing'] as side}
               <label>{side==='incoming'?'Вход':'Выход'}<select aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} ${side} mode`} value={point.tangents[side as 'incoming'|'outgoing'].mode} onchange={event=>{const tangents=point.tangents!;const mode=event.currentTarget.value as 'auto'|'free';edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tangents:{...tangents,[side]:{...tangents[side as 'incoming'|'outgoing'],mode}}}))}}><option value="auto">Автоматически</option><option value="free">Вручную</option></select></label>
               {#each ['x','y'] as coordinate}
                <label>{side==='incoming'?'Вход':'Выход'} {coordinate.toUpperCase()}<input type="number" step="0.01" aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} ${side} ${coordinate}`} value={point.tangents[side as 'incoming'|'outgoing'][coordinate as 'x'|'y']} onchange={event=>{const tangents=point.tangents!;edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tangents:{...tangents,[side]:{...tangents[side as 'incoming'|'outgoing'],mode:'free',[coordinate]:Number(event.currentTarget.value)}}}))}} /></label>
               {/each}
              {/each}
              <button class="btn ghost sm" aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} tangents clear`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tangents:undefined}))}>Убрать касательные</button>
             {:else}
              <button class="btn ghost sm" aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} tangents add`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,{tangents:resolvedKeyframeTangents(animation.track.keyframes,animation.track.keyframes.findIndex(key=>key.tick===point.tick),animation.track.interpolation)}))}>Касательные</button>
             {/if}
             <button class="btn ghost sm" aria-label={`${clip.id} mask ${index} ${channel} ${point.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,channel,point.tick,null))}>Удалить ключ</button>
            </div>
           {/each}
           <button class="btn ghost sm" aria-label={`${clip.id} mask ${index} ${channel} clear`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,[channel]:constant(value(animation,local))}))}>Удалить анимацию</button>
          </details>
         {/if}
        {/each}
        <label>Растушёвка<input aria-label={`${clip.id} mask ${index} feather`} type="number" min="0" max="1" step="0.01" value={effect.feather} onchange={event=>edit(track.id,clip.id,index,current=>({...current,feather:Number(event.currentTarget.value)}))} /></label>
        <label><input aria-label={`${clip.id} mask ${index} inverted`} type="checkbox" checked={effect.inverted} onchange={event=>edit(track.id,clip.id,index,current=>({...current,inverted:event.currentTarget.checked}))} />Инверсия</label>
        <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить маску</button>
       </div>
       <small>Координаты и размеры — доли исходного изображения. Изменение поля заменяет его анимацию постоянным значением.</small>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>.row{display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap}fieldset{border:0;padding:0.5rem 0;margin:0}input[type=number]{width:4.5rem}small{display:block}</style>
