<script lang="ts">
  import { tick } from 'svelte'
  import GraphicKeyTangents from './GraphicKeyTangents.svelte'
  import { nativeScalarTargetLabel,nativeScalarAddress,nativeScalarChannels,planNativeScalarKeyValues,planNativeScalarPointEdit,nativeScalarSelectionOperations,type NativeScalarKeySelection,type NativeScalarChannel } from '$lib/composition/nativeScalarTimeline'
  import { keyframeValueRange,keyframeCurvePoints,keyframeTailPoints,keyframeTangentPoints,keyframeTangentAtPoint,keyframeValueY,type KeyframeValueRange } from '$lib/composition/keyframeCurve'
  import { scalarEdgePanSpeed,combineScalarKeySelection,scalarKeysInBox,type ScalarTimeView,type ScalarSelectionMode,type ScalarScreenKey } from '$lib/composition/nativeScalarViewport'
  import { keyframeLocalTime } from '$lib/composition/keyframeClock'
  import { gridBoundaryTicks } from '$lib/composition/timeGrid'
  import { applyNativeKeyframeOperation } from '$lib/agent/nativeKeyframes'
  import { nestedComposition,nestedHistory,nestedAgentGeneration,editNestedScalarKeys } from '$lib/state/nestedComposition.svelte'
  import { nestedEffectPreview,clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
  import { authState } from '$lib/state/auth.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  import type {KeyframeTangentIntent} from '$lib/composition/tangents'
  import type {NativeNestedProject} from '$lib/api'
  let {selection,onselection,oncommitselection=onselection,onviewportwheel=()=>{},onviewportpan=()=>undefined,rangeStart,rangeEnd,disabled=false,onactivate=()=>{}}:{selection:readonly NativeScalarKeySelection[];onselection:(keys:NativeScalarKeySelection[])=>void;oncommitselection?:(keys:NativeScalarKeySelection[])=>void;onviewportwheel?:(event:WheelEvent,fraction:number,width:number)=>void;onviewportpan?:(delta:number,expected:ScalarTimeView)=>ScalarTimeView|undefined;rangeStart:number;rangeEnd:number;disabled?:boolean;onactivate?:()=>void}=$props()
  let graphOpen=$state(false),channelId=$state(''),mode=$state<'offset'|'multiply'|'set'>('offset'),operand=$state(0),pending=$state(false),message=$state(''),formVersion=$state(0),root=$state<HTMLElement>()
  const project=$derived(nestedComposition.document),scope=$derived(nestedComposition.activeCompositionId??project?.root??'')
  const channels=$derived(project?nativeScalarChannels(project,scope):[])
  const current=$derived(channels.find(channel=>channel.id===channelId)??channels.find(channel=>selection.some(key=>key.channelId===channel.id))??channels[0])
  const parameters=$derived(JSON.stringify([mode,operand,current?.id]))
  const unavailable=$derived(disabled||!!agentSession.lock||!authState.token)
  const blocked=$derived(unavailable||pending||nestedHistory.busy)
  const owner=Symbol('native scalar graph')
  type Context={base:NativeNestedProject;signature:string;scope:string;projectId:string|null;generation:number;token:string;time:number;selection:string;parameters:string;range:string;graph:boolean;controller:AbortController}
  type Plan={project:NativeNestedProject;operations:readonly Record<string,unknown>[];selection:NativeScalarKeySelection[]}
  type Drag={context:Context;pointer:number;element:SVGElement;rect:DOMRect;x:number;y:number;clientX:number;clientY:number;rangeStart:number;moved:boolean;channel:NativeScalarChannel;keys:NativeScalarKeySelection[];zoom:number;axis:KeyframeValueRange;kind:'key'|'tangent';tick:number;side?:'incoming'|'outgoing';origin?:{x:number;y:number}}
  let work=$state.raw<Context|null>(null),draft=$state.raw<{context:Context;plan:Plan}|null>(null),drag=$state.raw<Drag|null>(null),alive=true
  type Box={context:Context;pointer:number;element:SVGElement;rect:DOMRect;x:number;y:number;toX:number;toY:number;moved:boolean;axis:KeyframeValueRange;points:(ScalarScreenKey&{time:number})[];anchorTime:number;clientX:number;zoom:number;previous:NativeScalarKeySelection[];result:NativeScalarKeySelection[];mode:ScalarSelectionMode}
  let box=$state.raw<Box|null>(null)
  const shown=$derived(draft&&current?nativeScalarChannels(draft.plan.project,scope).find(channel=>channel.id===current.id)??current:current)
  const zoom=$derived(600/Math.max(1e-6,(rangeEnd-rangeStart)/1e6))
  const autoAxis=$derived.by(()=>{
    if(!shown)return {minimum:0,maximum:1}
    const range=keyframeValueRange(shown),span=range.maximum-range.minimum
    let minimum=range.minimum,maximum=range.maximum
    for(const point of keyframeTangentPoints(shown,zoom,160,range)){const value=range.maximum-point.y/160*span;minimum=Math.min(minimum,value);maximum=Math.max(maximum,value)}
    return Number.isFinite(minimum)&&Number.isFinite(maximum)?{minimum,maximum}:range
  })
  const axis=$derived(drag?.axis??box?.axis??autoAxis)
  const visibleHandles=$derived(shown?keyframeTangentPoints(shown,zoom,160,axis).filter(point=>{const time=point.x/zoom*1e6;return time>=rangeStart&&time<=rangeEnd||drag?.kind==='tangent'&&drag.tick===point.tick&&drag.side===point.side}):[])
  const displayedKeys=$derived.by(()=>{
    if(!current||!shown)return []
    const original=selection.filter(key=>key.channelId===current.id).map(key=>key.tick).sort((a,b)=>a-b),targets=draft?.plan.selection.filter(key=>key.channelId===current.id)
    return current.value.track.keyframes.filter(key=>{const time=current.start+keyframeLocalTime(current.value.track,key.tick);return time>=rangeStart&&time<=rangeEnd||drag?.kind==='key'&&drag.keys.some(item=>item.channelId===current.id&&item.tick===key.tick)}).map(key=>{const index=original.indexOf(key.tick),moving=draft?.plan.operations.some(op=>op.op==='move_keyframe'&&op.clipId===current.clipId&&op.property===current.property),at=moving&&index>=0?targets?.[index]?.tick??key.tick:key.tick;return {identity:key.tick,key:shown.value.track.keyframes.find(key=>key.tick===at)??key}})
  })
  const pointX=(channel:NativeScalarChannel,keyTick:number)=>(channel.start+keyframeLocalTime(channel.value.track,keyTick))/1e6*zoom
  const chosen=(keyTick:number)=>!!current&&(box?.moved?box.result:selection).some(key=>key.channelId===current.id&&key.tick===keyTick)
  function owns(context:Context){return alive&&work===context&&project===context.base&&JSON.stringify(project)===context.signature&&scope===context.scope&&nestedComposition.projectId===context.projectId&&nestedAgentGeneration()===context.generation&&authState.token===context.token&&nestedComposition.timeUs===context.time&&JSON.stringify(selection)===context.selection&&parameters===context.parameters&&JSON.stringify([rangeStart,rangeEnd])===context.range&&(!context.graph||graphOpen)&&!unavailable&&!context.controller.signal.aborted&&(!nestedEffectPreview.value||nestedEffectPreview.value.owner===owner)}
  let panFrame:number|undefined,panStamp=0
  function stopPan(){if(panFrame!==undefined)cancelAnimationFrame(panFrame);panFrame=undefined;panStamp=0}
  function cancel(){stopPan();const held=drag??box;work?.controller.abort();work=null;draft=null;drag=null;box=null;if(held?.element.hasPointerCapture?.(held.pointer))held.element.releasePointerCapture(held.pointer);clearEffectPreview(owner)}
  function begin(graph=false):Context|undefined{
    if(blocked||!project||!authState.token)return
    onactivate();cancel();message=''
    if(nestedEffectPreview.value&&nestedEffectPreview.value.owner!==owner){message='Завершите другое редактирование на кадре';return}
    const context={base:project,signature:JSON.stringify(project),scope,projectId:nestedComposition.projectId,generation:nestedAgentGeneration(),token:authState.token,time:nestedComposition.timeUs,selection:JSON.stringify(selection),parameters,range:JSON.stringify([rangeStart,rangeEnd]),graph,controller:new AbortController()};work=context;return context
  }
  $effect(()=>{const context=work;if(context&&(!owns(context)||nestedHistory.busy&&!pending))cancel()})
  $effect(()=>()=>{alive=false;cancel()})
  function publish(context:Context,plan:Plan){if(!owns(context))return;draft={context,plan};nestedEffectPreview.value={owner,gesture:context,base:context.base,project:plan.project,timeUs:context.time,compositionId:context.scope,token:context.token}}
  async function apply(prepared=draft){
    if(!prepared||blocked||!owns(prepared.context))return
    if(!prepared.plan.operations.length||JSON.stringify(prepared.plan.project)===prepared.context.signature){cancel();message='Значения не изменились';return}
    const context=prepared.context,focus=root?.contains(document.activeElement)||document.activeElement===document.body;pending=true
    try{
      const accepted=await editNestedScalarKeys(context.scope,prepared.plan.operations,()=>{if(!owns(context))throw new Error('Контекст графика изменился')},context.controller.signal)
      if(accepted){oncommitselection(prepared.plan.selection);pending=false;await tick();if(alive&&focus&&(root?.contains(document.activeElement)||document.activeElement===document.body)){const key=prepared.plan.selection.find(key=>key.channelId===current?.id);[...(root?.querySelectorAll<SVGElement>('[data-native-graph-key]')??[])].find(element=>element.dataset.nativeGraphKey===String(key?.tick))?.focus()}}
      else if(work===context)message=nestedHistory.error||'Изменение не применено'
    }catch(error){if(work===context)message=error instanceof Error?error.message:String(error)}
    finally{pending=false;if(alive)formVersion++;if(work===context)cancel()}
  }
  function previewValues(){
    if(!selection.length)return
    const context=begin();if(!context)return
    try{const plan=planNativeScalarKeyValues(context.base,context.scope,selection,mode,operand);if(!plan.operations.length){cancel();message='Значения не изменились';return}publish(context,plan)}catch(error){cancel();message=error instanceof Error?error.message:String(error)}
  }
  function graphSelection(keyTick:number,toggle=false){
    if(!current)return []
    let keys=selection.filter(key=>key.channelId===current.id)
    if(toggle)keys=keys.some(key=>key.tick===keyTick)?keys.filter(key=>key.tick!==keyTick):[...keys,{channelId:current.id,tick:keyTick}]
    else if(!keys.some(key=>key.tick===keyTick))keys=[{channelId:current.id,tick:keyTick}]
    onselection(keys);return keys
  }
  function down(event:PointerEvent,keyTick:number,side?:'incoming'|'outgoing'){
    if(blocked||current?.locked||!current||event.button!==0||drag||box)return
    event.preventDefault();event.stopPropagation()
    const keys=side?[...selection]:graphSelection(keyTick,event.shiftKey||event.ctrlKey||event.metaKey)
    if(!side&&!keys.some(key=>key.tick===keyTick))return
    const element=event.currentTarget as SVGElement,rect=element.ownerSVGElement!.getBoundingClientRect()
    if(!rect.width||!rect.height)return
    const context=begin(true);if(!context)return
    const origin=side?keyframeTangentPoints(current,zoom,160,axis).find(point=>point.tick===keyTick&&point.side===side):undefined
    element.setPointerCapture?.(event.pointerId)
    drag={context,pointer:event.pointerId,element,rect,x:event.clientX,y:event.clientY,clientX:event.clientX,clientY:event.clientY,rangeStart,moved:false,channel:current,keys,zoom,axis:{...axis},kind:side?'tangent':'key',tick:keyTick,side,origin}
  }
  function boxDown(event:PointerEvent){
    if(blocked||!current||current.locked||event.button!==0||drag||box)return
    const element=event.currentTarget as SVGElement,rect=element.ownerSVGElement!.getBoundingClientRect()
    if(!rect.width||!rect.height)return
    event.preventDefault();event.stopPropagation()
    const context=begin(true);if(!context)return
    const points=current.value.track.keyframes.map(key=>({key:{channelId:current.id,tick:key.tick},time:current.start+keyframeLocalTime(current.value.track,key.tick),x:rect.left+(20+pointX(current,key.tick)-rangeStart/1e6*zoom)/640*rect.width,y:rect.top+(10+keyframeValueY(key.value,axis,160))/180*rect.height}))
    box={context,pointer:event.pointerId,element,rect,x:event.clientX,y:event.clientY,toX:event.clientX,toY:event.clientY,moved:false,axis:{...axis},points,anchorTime:rangeStart+((event.clientX-rect.left)/rect.width*640-20)/zoom*1e6,clientX:event.clientX,zoom,previous:[...selection],result:[...selection],mode:event.altKey?'subtract':event.ctrlKey||event.metaKey?'toggle':event.shiftKey?'add':'replace'}
    element.setPointerCapture?.(event.pointerId)
  }
  function gestureValid(held:Drag|Box){
    const rect=held.element.ownerSVGElement?.getBoundingClientRect()
    return owns(held.context)&&!nestedHistory.busy&&held.element.isConnected&&!!rect&&(['left','top','width','height'] as const).every(key=>Math.abs(rect[key]-held.rect[key])<.5)
  }
  function publishBox(held:Box){
    held.x=held.rect.left+(20+(held.anchorTime-rangeStart)/1e6*held.zoom)/640*held.rect.width
    const points=held.points.map(point=>({...point,x:held.rect.left+(20+(point.time-rangeStart)/1e6*held.zoom)/640*held.rect.width}))
    held.result=held.moved?combineScalarKeySelection(held.previous,scalarKeysInBox(points,held.x,held.y,held.toX,held.toY),held.mode):held.previous;box={...held}
  }
  function boxMove(event:PointerEvent){
    const held=box;if(!held||held.pointer!==event.pointerId)return
    if(!gestureValid(held)){cancel();return}
    held.moved ||= Math.hypot(event.clientX-held.x,event.clientY-held.y)>=3
    held.clientX=event.clientX;held.toX=Math.max(held.rect.left,Math.min(held.rect.right,event.clientX));held.toY=Math.max(held.rect.top,Math.min(held.rect.bottom,event.clientY))
    publishBox(held);event.preventDefault();schedulePan(held)
  }
  function schedulePan(held:Drag|Box){
    const left=held.rect.left+20/640*held.rect.width,width=600/640*held.rect.width
    if(!held.moved||!scalarEdgePanSpeed(held.clientX,left,width)){stopPan();return}
    if(panFrame!==undefined)return
    panStamp=performance.now();panFrame=requestAnimationFrame(autoPan)
  }
  function autoPan(stamp:number){
    panFrame=undefined
    const held=drag??box;if(!held)return
    if(!gestureValid(held)){cancel();return}
    const speed=scalarEdgePanSpeed(held.clientX,held.rect.left+20/640*held.rect.width,600/640*held.rect.width),elapsed=Math.max(0,Math.min(50,stamp-panStamp));panStamp=stamp
    if(!speed)return
    const before={start:rangeStart,end:rangeEnd},next=onviewportpan(speed*(rangeEnd-rangeStart)*elapsed/1000,before)
    if(!next){cancel();return}
    if(next.start===before.start&&next.end===before.end)return
    held.context.range=JSON.stringify([next.start,next.end])
    if(drag)publishDrag(drag);else publishBox(held as Box)
    panFrame=requestAnimationFrame(autoPan)
  }
  function boxUp(event:PointerEvent){
    if(box?.pointer!==event.pointerId)return
    boxMove(event);const held=box;if(!held)return
    const result=held.moved?held.result:combineScalarKeySelection(held.previous,[],held.mode)
    cancel();onselection(result)
  }
  function tangentPlan(context:Context,channel:NativeScalarChannel,keyTick:number,intent:KeyframeTangentIntent):Plan{
    const base={...nativeScalarAddress(channel)}
    const operation=intent.kind==='mode'?{...base,op:'set_keyframe_tangent_mode',ticks:[keyTick],mode:intent.mode}:intent.kind==='continuity'?{...base,op:'edit_keyframe_tangents',tick:keyTick,continuity:intent.continuity}:{...base,op:'edit_keyframe_tangents',tick:keyTick,[intent.side]:{x:intent.x,y:intent.y}}
    return {project:applyNativeKeyframeOperation(context.base,context.scope,operation),operations:[operation],selection:[...selection]}
  }
  function publishDrag(held:Drag){
    const dx=held.clientX-held.x,dy=held.clientY-held.y
    try{
      let plan:Plan
      if(held.kind==='key')plan=planNativeScalarPointEdit(held.context.base,held.context.scope,held.keys,Math.round(dx*640/held.rect.width/held.zoom*1e6+rangeStart-held.rangeStart),-dy*180/held.rect.height/160*(held.axis.maximum-held.axis.minimum))
      else{
        const point=keyframeTangentAtPoint(held.channel,held.tick,held.side!,{x:held.origin!.x+dx*640/held.rect.width+(rangeStart-held.rangeStart)/1e6*held.zoom,y:held.origin!.y+dy*180/held.rect.height},held.zoom,160,held.axis)
        if(!point)throw new Error('Эта ручка недоступна')
        plan=tangentPlan(held.context,held.channel,held.tick,{kind:'point',side:held.side!,...point})
      }
      drag={...held};publish(held.context,plan);message=''
    }catch(error){draft=null;clearEffectPreview(owner);message=error instanceof Error?error.message:String(error)}
  }
  function move(event:PointerEvent){
    if(box){boxMove(event);return}
    const held=drag;if(!held||held.pointer!==event.pointerId)return
    if(!gestureValid(held)){cancel();return}
    held.clientX=event.clientX;held.clientY=event.clientY;held.moved ||= Math.hypot(event.clientX-held.x,event.clientY-held.y)>=3
    if(!held.moved)return
    event.preventDefault();publishDrag(held);schedulePan(held)
  }
  function up(event:PointerEvent){
    if(box){boxUp(event);return}
    const held=drag;if(!held||held.pointer!==event.pointerId)return
    move(event);if(drag?.context!==held.context)return;stopPan();const prepared=draft;drag=null
    if(held.element.hasPointerCapture?.(event.pointerId))held.element.releasePointerCapture(event.pointerId)
    if(!held.moved||!prepared){cancel();return}
    void apply(prepared)
  }
  function tangentEdit(keyTick:number,intent:KeyframeTangentIntent){
    if(!current||current.locked)return
    const context=begin(true);if(!context)return
    try{const plan=tangentPlan(context,current,keyTick,intent);publish(context,plan);void apply({context,plan})}catch(error){cancel();message=error instanceof Error?error.message:String(error)}
  }
  function keyboard(event:KeyboardEvent,keyTick:number,side?:'incoming'|'outgoing'){
    if(blocked||!current||current.locked)return
    if(event.key==='Escape'){cancel();return}
    if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='a'){event.preventDefault();event.stopPropagation();onselection(current.value.track.keyframes.map(key=>({channelId:current.id,tick:key.tick})));return}
    if(!side&&(event.key==='Delete'||event.key==='Backspace')){
      event.preventDefault();event.stopPropagation();const keys=graphSelection(keyTick),context=begin(true);if(!context)return
      try{const operations=nativeScalarSelectionOperations(context.base,context.scope,keys,'delete');let candidate=context.base;for(const op of operations)candidate=applyNativeKeyframeOperation(candidate,context.scope,op);const plan={project:candidate,operations,selection:[]};publish(context,plan);void apply({context,plan})}catch(error){cancel();message=error instanceof Error?error.message:String(error)}
      return
    }
    if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key))return
    event.preventDefault();event.stopPropagation()
    if(side){const handle=keyframeTangentPoints(current,zoom,160,autoAxis).find(point=>point.tick===keyTick&&point.side===side);if(!handle)return;const point=keyframeTangentAtPoint(current,keyTick,side,{x:handle.x+(event.key==='ArrowLeft'?-1:event.key==='ArrowRight'?1:0)*(event.shiftKey?10:1),y:handle.y+(event.key==='ArrowUp'?-1:event.key==='ArrowDown'?1:0)*(event.shiftKey?10:1)},zoom,160,autoAxis);if(point)tangentEdit(keyTick,{kind:'point',side,...point});return}
    const keys=graphSelection(keyTick),context=begin(true);if(!context)return
    try{
      const canvas=context.base.compositions[scope]!.content.canvas,rate=canvas.fpsRate??{numerator:canvas.fpsMilli,denominator:1000},amount=event.shiftKey?10:1
      const delta=event.key==='ArrowLeft'||event.key==='ArrowRight'?(event.key==='ArrowLeft'?-1:1)*gridBoundaryTicks(amount,rate):0
      const step=current.property==='opacity'||current.property==='pan'||current.property==='progress'||current.envelope==='effect'&&(current.property.startsWith('colorAnimation.')||/^lumetri\.grade\.(?:shadows|midtones|highlights)[XY]$/.test(current.property))?0.01:1
      const value=event.key==='ArrowUp'||event.key==='ArrowDown'?(event.key==='ArrowUp'?1:-1)*amount*step:0
      const plan=planNativeScalarPointEdit(context.base,context.scope,keys,delta,value);publish(context,plan);void apply({context,plan})
    }catch(error){cancel();message=error instanceof Error?error.message:String(error)}
  }
</script>
<svelte:window onresize={()=>{if(box||drag)cancel()}} onkeydown={event=>{if(event.key==='Escape'&&work)cancel()}} />
{#if channels.length}
  <div bind:this={root}>
    <fieldset disabled={blocked}>
      <legend>Значения выделенных ключей</legend>
      <label>Действие<select aria-label="Действие со значениями нативных ключей" value={mode} onchange={event=>mode=event.currentTarget.value as typeof mode}><option value="offset">Прибавить</option><option value="multiply">Умножить</option><option value="set">Задать</option></select></label>
      <label>Число<input aria-label="Число для значений нативных ключей" type="number" step="any" bind:value={operand} /></label>
      <button type="button" class="btn ghost sm" disabled={!selection.length} onclick={previewValues}>Предпросмотр значений ключей</button>
      <button type="button" class="btn ghost sm" disabled={!draft||!owns(draft.context)} onclick={()=>void apply()}>Применить значения ключей</button>
    </fieldset>
    {#if work}<button type="button" class="btn ghost sm" onclick={cancel}>Отменить изменение графика</button>{/if}
    <details bind:open={graphOpen}>
      <summary>График значений и кривых</summary>
      <label>Канал<select aria-label="Канал графика нативных ключей" value={current?.id??''} disabled={blocked} onchange={event=>channelId=event.currentTarget.value}>{#each channels as channel (channel.id)}<option value={channel.id}>{channel.label}</option>{/each}</select></label>
      {#if graphOpen&&current&&shown}
        <p>{axis.minimum.toFixed(3)}–{axis.maximum.toFixed(3)} · {(rangeStart/1e6).toFixed(3)}–{(rangeEnd/1e6).toFixed(3)} с</p>
        <svg viewBox="0 0 640 180" preserveAspectRatio="none" role="group" aria-label="График нативных скалярных ключей" onwheel={event=>{const rect=event.currentTarget.getBoundingClientRect();if(rect.width>0)onviewportwheel(event,((event.clientX-rect.left)/rect.width*640-20)/600,rect.width*600/640)}} onpointermove={move} onpointerup={up} onpointercancel={event=>{if(drag?.pointer===event.pointerId||box?.pointer===event.pointerId)cancel()}}>
          <rect width="640" height="180" fill="transparent" role="button" tabindex={blocked||current.locked?-1:0} aria-label="Выделить рамкой на графике" aria-disabled={blocked||current.locked} onpointerdown={boxDown} onlostpointercapture={event=>{if(box?.element===event.currentTarget)cancel()}} onkeydown={event=>{if((event.key==='Enter'||event.key===' ')&&!blocked&&!current.locked){event.preventDefault();const context=begin(true);if(context){cancel();onselection(displayedKeys.map(entry=>({channelId:current.id,tick:entry.identity})))}}}} />
          <g transform={`translate(${20-rangeStart/1e6*zoom} 10)`}>
            <polyline pointer-events="none" data-curve="before" points={keyframeTailPoints(shown,'before',zoom,160,axis)} fill="none" stroke="currentColor" stroke-dasharray="3 3" />
            <polyline pointer-events="none" data-curve="keys" points={keyframeCurvePoints(shown,zoom,160,axis)} fill="none" stroke="currentColor" />
            <polyline pointer-events="none" data-curve="after" points={keyframeTailPoints(shown,'after',zoom,160,axis)} fill="none" stroke="currentColor" stroke-dasharray="3 3" />
            {#each visibleHandles as handle (`${handle.tick}/${handle.side}`)}
              <line pointer-events="none" x1={handle.keyX} y1={handle.keyY} x2={handle.x} y2={handle.y} stroke="currentColor" stroke-dasharray="2 2" />
              <circle role="button" tabindex={blocked||current.locked?-1:0} aria-disabled={blocked||current.locked} aria-label={`Ручка графика ${nativeScalarTargetLabel(current)} ${handle.tick} ${handle.side}`} cx={handle.x} cy={handle.y} r="3" fill="var(--panel-2,#222)" stroke="currentColor" onpointerdown={event=>down(event,handle.tick,handle.side)} onlostpointercapture={event=>{if(drag?.element===event.currentTarget)cancel()}} onkeydown={event=>keyboard(event,handle.tick,handle.side)} />
            {/each}
            {#each displayedKeys as entry (entry.identity)}
              <circle role="button" tabindex={blocked||current.locked?-1:0} aria-disabled={blocked||current.locked} aria-pressed={chosen(entry.identity)} data-native-graph-key={entry.identity} aria-label={`Точка графика ${nativeScalarTargetLabel(current)} ${entry.identity}`} cx={pointX(shown,entry.key.tick)} cy={keyframeValueY(entry.key.value,axis,160)} r="4" fill={chosen(entry.identity)?'var(--accent,#6af)':'var(--panel-2,#222)'} stroke="currentColor" onpointerdown={event=>down(event,entry.identity)} onlostpointercapture={event=>{if(drag?.element===event.currentTarget)cancel()}} onkeydown={event=>keyboard(event,entry.identity)}><title>{entry.key.value} · {((shown.start+keyframeLocalTime(shown.value.track,entry.key.tick))/1e6).toFixed(3)} с</title></circle>
            {/each}
          </g>
          {#if box?.moved}<rect pointer-events="none" x={(Math.min(box.x,box.toX)-box.rect.left)/box.rect.width*640} y={(Math.min(box.y,box.toY)-box.rect.top)/box.rect.height*180} width={Math.abs(box.toX-box.x)/box.rect.width*640} height={Math.abs(box.toY-box.y)/box.rect.height*180} fill="var(--accent,#6af)" fill-opacity=".15" stroke="var(--accent,#6af)" />{/if}
        </svg>
        <p>У края графика время прокручивается при перетаскивании точки, ручки или рамки. Рамка выделяет точки по времени и значению. Shift добавляет, Ctrl/Meta переключает, Alt исключает. Точка меняет время и значение. Стрелки: кадр по горизонтали, шаг значения по вертикали; Shift увеличивает шаг. Escape отменяет жест.</p>
        <fieldset disabled={blocked||current.locked}>
          {#key formVersion}
          {#each current.value.track.keyframes.filter(key=>chosen(key.tick)) as key (key.tick)}
            <GraphicKeyTangents animation={current.value} tick={key.tick} label={`График ${nativeScalarTargetLabel(current)} ${key.tick}`} onintent={intent=>tangentEdit(key.tick,intent)} />
          {/each}
          {/key}
        </fieldset>
      {/if}
    </details>
    {#if message}<p aria-live="polite">{message}</p>{/if}
  </div>
{/if}
<style>fieldset{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap;border:0;padding:.5rem 0}label{display:flex;gap:.3rem;margin:.4rem 0}input{width:7rem}select{max-width:24rem}svg{width:100%;height:180px;touch-action:none;overflow:hidden}circle{cursor:move}p{font-size:.8rem}summary{cursor:pointer}</style>
