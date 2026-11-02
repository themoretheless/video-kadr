<script lang="ts">
  import { tick } from 'svelte'
  import {registerNativeScalarHistoryView} from '$lib/composition/nativeScalarHistoryView'
  import NestedScalarClipboard from './NestedScalarClipboard.svelte'
  import NestedScalarGraph from './NestedScalarGraph.svelte'
  let clipboardControls=$state<{copy:(cut?:boolean)=>Promise<void>;paste:()=>Promise<void>}>()
  import { nestedComposition,nestedHistory,nestedPlayback,nestedAgentGeneration,editNestedScalarKeys } from '$lib/state/nestedComposition.svelte'
  import { nestedEffectPreview,clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
  import { authState } from '$lib/state/auth.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  import { nativeScalarTargetLabel,nativeScalarAddress,nativeScalarChannels,planNativeScalarKeyMove,planNativeScalarKeyScale,nativeScalarSelectionOperations,type NativeScalarKeySelection,type NativeScalarChannel } from '$lib/composition/nativeScalarTimeline'
  import { keyframeLocalTime,keyframeAuthoredTick } from '$lib/composition/keyframeClock'
  import { sampleAnimatableValue } from '$lib/composition/keyframes'
  import { gridBoundaryTicks } from '$lib/composition/timeGrid'
  import {scalarEdgePanSpeed,nativeScalarExtent,constrainScalarTimeView,zoomScalarTimeView,panScalarTimeView,scalarKeysInTimeRange,scalarKeysInBox,combineScalarKeySelection,type ScalarTimeView,type ScalarSelectionMode,type ScalarScreenKey} from '$lib/composition/nativeScalarViewport'
  import {nativeClipStart} from '$lib/composition/nativeClipMove'
  import {nativeClipEnd} from '$lib/composition/nativeClipInsert'
  import { EASING_PRESETS } from '$lib/composition/easingPresets'
  import type { NativeNestedProject } from '$lib/api'
  let { disabled=false }:{disabled?:boolean}=$props()
  let root=$state<HTMLElement>(),selection=$state.raw<NativeScalarKeySelection[]>([]),pending=$state(false),error=$state('')
  let scaleFactor=$state(2),scaleAnchor=$state<'first'|'last'|'cursor'>('first')
  const scaleParameters=$derived(JSON.stringify([scaleFactor,scaleAnchor]))
  const project=$derived(nestedComposition.document),scope=$derived(nestedComposition.activeCompositionId??project?.root??'')
  const channels=$derived(project?nativeScalarChannels(project,scope):[])
  let view=$state.raw<ScalarTimeView|null>(null),viewFrom=$state<number|undefined>(),viewTo=$state<number|undefined>(),selectionFrom=$state<number|undefined>(),selectionTo=$state<number|undefined>(),selectionMode=$state<ScalarSelectionMode>('replace'),lanes=$state<HTMLElement>()
  const extent=$derived(nativeScalarExtent(channels,project?.compositions[scope]?.content.tracks.flatMap(track=>track.clips.map(clip=>({start:nativeClipStart(clip),end:nativeClipEnd(clip)})))??[]))
  const visible=$derived(view?constrainScalarTimeView(view,extent):extent)
  const first=$derived(visible.start),last=$derived(visible.end)
  const rate=$derived(project?.compositions[scope]?.content.canvas.fpsRate??{numerator:project?.compositions[scope]?.content.canvas.fpsMilli??30000,denominator:1000})
  const unavailable=$derived(disabled||!!agentSession.lock||!authState.token)
  const blocked=$derived(unavailable||pending||nestedHistory.busy)
  const owner=Symbol('native scalar timeline')
  type Context={base:NativeNestedProject;signature:string;scope:string;token:string;generation:number;projectId:string|null;time:number;selection:string;view:string;controller:AbortController}
  type Drag={context:Context;pointer:number;svg:HTMLButtonElement;x:number;width:number;first:number;last:number;rect:DOMRect;clientX:number;moved:boolean;plan?:ReturnType<typeof planNativeScalarKeyMove>}
  type Marquee={context:Context;pointer:number;element:HTMLButtonElement;rect:DOMRect;wrapper:DOMRect;x:number;y:number;toX:number;toY:number;points:(ScalarScreenKey&{time:number;left:number;width:number})[];rows:{element:HTMLElement;rect:DOMRect}[];anchorUs:number;clientX:number;mode:ScalarSelectionMode;moved:boolean;result:NativeScalarKeySelection[];top:number;bottom:number}
  let marquee=$state.raw<Marquee|null>(null)
  const displaySelection=$derived(marquee?.moved?marquee.result:selection)
  let scalePreview=$state.raw<{context:Context;plan:ReturnType<typeof planNativeScalarKeyScale>;parameters:string;anchor:number}|null>(null)
  let work=$state.raw<Context|null>(null),drag=$state.raw<Drag|null>(null),alive=true,selectionContext='',suppressClick=false
  const same=(a:NativeScalarKeySelection,b:NativeScalarKeySelection)=>a.channelId===b.channelId&&a.tick===b.tick
  const selectedIds=$derived(new Set(displaySelection.map(key=>JSON.stringify([key.channelId,key.tick]))))
  const selected=(key:NativeScalarKeySelection)=>selectedIds.has(JSON.stringify([key.channelId,key.tick]))
  const address=(channel:NativeScalarChannel,tick:number):NativeScalarKeySelection=>({channelId:channel.id,tick})
  function owns(context:Context){return alive&&work===context&&project===context.base&&JSON.stringify(project)===context.signature&&scope===context.scope&&authState.token===context.token&&nestedAgentGeneration()===context.generation&&nestedComposition.projectId===context.projectId&&nestedComposition.timeUs===context.time&&JSON.stringify(selection)===context.selection&&JSON.stringify([first,last])===context.view&&!unavailable&&(!scalePreview||scalePreview.context!==context||scalePreview.parameters===scaleParameters)&&!context.controller.signal.aborted&&(!nestedEffectPreview.value||nestedEffectPreview.value.owner===owner)}
  let panFrame:number|undefined,panStamp=0
  function stopPan(){if(panFrame!==undefined)cancelAnimationFrame(panFrame);panFrame=undefined;panStamp=0}
  function cancel(){stopPan();const held=drag,range=marquee;work?.controller.abort();work=null;drag=null;marquee=null;scalePreview=null;if(held?.svg.hasPointerCapture?.(held.pointer))held.svg.releasePointerCapture(held.pointer);if(range?.element.hasPointerCapture?.(range.pointer))range.element.releasePointerCapture(range.pointer);clearEffectPreview(owner)}
  const viewIdentity=()=>JSON.stringify([scope,nestedComposition.projectId,nestedAgentGeneration(),authState.token])
  $effect(()=>registerNativeScalarHistoryView({capture:()=>({context:viewIdentity(),selection:[...selection],view}),restore:value=>{cancel();selection=value.selection;view=value.view;viewFrom=undefined;viewTo=undefined}}))
  $effect(()=>{
    const identity=viewIdentity()
    if(identity!==selectionContext){selectionContext=identity;selection=[];view=null;viewFrom=undefined;viewTo=undefined;selectionFrom=undefined;selectionTo=undefined;cancel()}
    else if(!work){const valid=selection.filter(item=>channels.some(channel=>channel.id===item.channelId&&channel.value.track.keyframes.some(key=>key.tick===item.tick)));if(valid.length!==selection.length)selection=valid}
  })
  $effect(()=>{const context=work;if(context&&(!owns(context)||nestedHistory.busy&&!pending))cancel()})
  $effect(()=>{if(scalePreview&&scalePreview.parameters!==scaleParameters)cancel()})
  $effect(()=>()=>{alive=false;cancel()})
  function context():Context|undefined{
    if(!project||!authState.token||blocked)return
    if(nestedEffectPreview.value&&nestedEffectPreview.value.owner!==owner){error='Завершите другое редактирование на кадре';return}
    cancel();error=''
    const result={base:project,signature:JSON.stringify(project),scope,token:authState.token,generation:nestedAgentGeneration(),projectId:nestedComposition.projectId,time:nestedComposition.timeUs,selection:JSON.stringify(selection),view:JSON.stringify([first,last]),controller:new AbortController()};work=result;return result
  }
  function choose(key:NativeScalarKeySelection,toggle=false){
    if(blocked)return
    if(toggle)selection=selected(key)?selection.filter(item=>!same(item,key)):[...selection,key]
    else if(!selected(key))selection=[key]
  }
  function click(event:MouseEvent,channel:NativeScalarChannel,keyTick:number){
    if(suppressClick){suppressClick=false;return}
    if(blocked||channel.locked)return
    if(!event.detail)choose(address(channel,keyTick),event.shiftKey||event.ctrlKey||event.metaKey)
    nestedPlayback.playing=false;nestedPlayback.epoch++
    nestedComposition.timeUs=Math.max(0,Math.round(channel.start+keyframeLocalTime(channel.value.track,keyTick)))
  }
  function down(event:PointerEvent,channel:NativeScalarChannel,keyTick:number){
    if(event.button!==0||blocked||channel.locked||drag||marquee)return
    const key=address(channel,keyTick);choose(key,event.shiftKey||event.ctrlKey||event.metaKey)
    if(!selected(key))return
    const element=event.currentTarget as HTMLButtonElement,rect=element.parentElement!.getBoundingClientRect(),width=rect.width
    if(width<=0)return
    const held=context();if(!held)return
    suppressClick=false;element.setPointerCapture?.(event.pointerId)
    drag={context:held,pointer:event.pointerId,svg:element,x:event.clientX,width,first,last,rect,clientX:event.clientX,moved:false}
  }
  function dragValid(held:Drag){
    const rect=held.svg.parentElement?.getBoundingClientRect()
    return owns(held.context)&&!nestedHistory.busy&&!!rect&&(['left','top','width','height'] as const).every(key=>Math.abs(rect[key]-held.rect[key])<.5)
  }
  function publishDrag(held:Drag){
    const origin=held.first+(held.x-held.rect.left)/held.width*(held.last-held.first)
    const at=first+(held.clientX-held.rect.left)/held.width*(last-first)
    try{
      held.plan=planNativeScalarKeyMove(held.context.base,held.context.scope,selection,Math.round(at-origin))
      drag={...held};error=''
      nestedEffectPreview.value={owner,gesture:held.context,base:held.context.base,project:held.plan.project,timeUs:held.context.time,compositionId:held.context.scope,token:held.context.token}
    }catch(cause){held.plan=undefined;drag={...held};clearEffectPreview(owner);error=cause instanceof Error?cause.message:String(cause)}
  }
  function startPan(){
    if(panFrame!==undefined||!drag?.moved||!scalarEdgePanSpeed(drag.clientX,drag.rect.left,drag.width))return
    panStamp=performance.now();panFrame=requestAnimationFrame(autoPan)
  }
  function autoPan(stamp:number){
    panFrame=undefined
    const held=drag;if(!held)return
    if(!dragValid(held)){cancel();return}
    const speed=scalarEdgePanSpeed(held.clientX,held.rect.left,held.width),elapsed=Math.max(0,Math.min(50,stamp-panStamp));panStamp=stamp
    if(!speed)return
    const next=panScalarTimeView(visible,extent,speed*(last-first)*elapsed/1000)
    if(next.start===first&&next.end===last)return
    view=next;held.context.view=JSON.stringify([next.start,next.end]);publishDrag(held)
    panFrame=requestAnimationFrame(autoPan)
  }
  function move(event:PointerEvent){
    const held=drag;if(!held||held.pointer!==event.pointerId)return
    if(!dragValid(held)){cancel();return}
    held.clientX=event.clientX;held.moved ||= Math.abs(event.clientX-held.x)>=3
    if(!held.moved)return
    event.preventDefault();publishDrag(held)
    if(scalarEdgePanSpeed(held.clientX,held.rect.left,held.width))startPan();else stopPan()
  }
  async function commit(held:Context,operations:readonly Record<string,unknown>[],next:NativeScalarKeySelection[]){
    if(!owns(held)||blocked||!operations.length){cancel();return}
    pending=true
    const focus=root?.contains(document.activeElement)||document.activeElement===document.body
    try{
      const accepted=await editNestedScalarKeys(held.scope,operations,()=>{if(!owns(held))throw new Error('Контекст ключей изменился')},held.controller.signal)
      if(accepted){acceptEditedSelection(next);pending=false;await tick();if(focus&&alive&&(root?.contains(document.activeElement)||document.activeElement===document.body)){const target=[...(root?.querySelectorAll<HTMLButtonElement>('[data-native-key]')??[])].find(button=>button.dataset.nativeKey===JSON.stringify(next[0]));target?.focus({preventScroll:true})}}
      else if(work===held)error=nestedHistory.error||'Изменение не применено'
    }catch(cause){if(work===held)error=cause instanceof Error?cause.message:String(cause)}
    finally{pending=false;if(work===held)cancel()}
  }
  function up(event:PointerEvent){
    const held=drag;if(!held||held.pointer!==event.pointerId)return
    move(event);if(drag?.context!==held.context)return;stopPan();drag=null
    if(held.svg.hasPointerCapture?.(event.pointerId))held.svg.releasePointerCapture(event.pointerId)
    if(!held.moved){cancel();return}
    suppressClick=true
    const plan=held.plan
    if(!plan||!owns(held.context)){cancel();return}
    void commit(held.context,plan.operations,plan.selection)
  }
  function nudge(event:KeyboardEvent,channel:NativeScalarChannel,keyTick:number){
    if(blocked||channel.locked)return
    if(event.key==='Escape'){cancel();return}
    if(clipboardShortcut(event)){choose(address(channel,keyTick));return}
    if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='a'){event.preventDefault();selectAll();return}
    if(event.key==='Delete'||event.key==='Backspace'){event.preventDefault();choose(address(channel,keyTick));void action('delete');return}
    if(event.key!=='ArrowLeft'&&event.key!=='ArrowRight')return
    event.preventDefault();choose(address(channel,keyTick))
    const held=context();if(!held)return
    try{const plan=planNativeScalarKeyMove(held.base,scope,selection,(event.key==='ArrowLeft'?-1:1)*gridBoundaryTicks(event.shiftKey?10:1,rate));void commit(held,plan.operations,plan.selection)}catch(cause){cancel();error=cause instanceof Error?cause.message:String(cause)}
  }
  function clipboardShortcut(event:KeyboardEvent){
    const key=event.key.toLowerCase()
    if(!(event.ctrlKey||event.metaKey)||!['c','x','v'].includes(key)||blocked)return false
    event.preventDefault();event.stopPropagation();cancel()
    void tick().then(()=>key==='v'?clipboardControls?.paste():clipboardControls?.copy(key==='x'))
    return true
  }
  const keyTime=(channel:NativeScalarChannel,keyTick:number)=>channel.start+keyframeLocalTime(channel.value.track,keyTick)
  function panGraphView(delta:number,expected:ScalarTimeView):ScalarTimeView|undefined{
    if(blocked||work||first!==expected.start||last!==expected.end||!Number.isFinite(delta))return
    const next=panScalarTimeView(visible,extent,delta);view=next;return next
  }
  function setView(next:ScalarTimeView|null){
    if(blocked)return
    try{const validated=next?constrainScalarTimeView(next,extent):null;cancel();view=validated;viewFrom=undefined;viewTo=undefined;error=''}catch(cause){error=cause instanceof Error?cause.message:String(cause)}
  }
  function acceptEditedSelection(next:NativeScalarKeySelection[]){
    selection=next
    if(!view||!next.length)return
    const firstKey=next[0]!,channel=channels.find(channel=>channel.id===firstKey.channelId)
    if(!channel)return
    const time=keyTime(channel,firstKey.tick)
    if(time<first||time>last){const half=(last-first)/2;view=constrainScalarTimeView({start:time-half,end:time+half},extent)}
  }
  function zoomView(factor:number){try{setView(zoomScalarTimeView(visible,extent,factor,nestedComposition.timeUs))}catch(cause){error=String(cause)}}
  function panView(direction:number){setView(panScalarTimeView(visible,extent,direction*(last-first)/2))}
  function wheelView(event:WheelEvent,position?:{fraction:number;width:number}){
    if(!(event.ctrlKey||event.metaKey||event.shiftKey||event.deltaX)||!Number.isFinite(event.deltaX)||!Number.isFinite(event.deltaY)||event.deltaX===0&&event.deltaY===0)return
    const rect=(event.currentTarget as HTMLElement).getBoundingClientRect(),width=position?.width??rect.width
    if(width<=0)return
    event.preventDefault();event.stopPropagation();if(blocked)return
    const unit=event.deltaMode===1?16:event.deltaMode===2?width:1,delta=(event.deltaX||event.deltaY)*unit
    if(event.ctrlKey||event.metaKey){const fraction=Math.max(0,Math.min(1,position?.fraction??(event.clientX-rect.left)/width));setView(zoomScalarTimeView(visible,extent,Math.exp(-Math.max(-1000,Math.min(1000,event.deltaY*unit))*0.002),first+fraction*(last-first)))}
    else setView(panScalarTimeView(visible,extent,delta/width*(last-first)))
  }
  function fitSelection(){
    const times=selection.flatMap(key=>{const channel=channels.find(channel=>channel.id===key.channelId);return channel?[keyTime(channel,key.tick)]:[]})
    if(!times.length)return
    let low=Infinity,high=-Infinity;for(const time of times){low=Math.min(low,time);high=Math.max(high,time)}
    const half=Math.max(gridBoundaryTicks(1,rate),high-low)*0.6,center=(low+high)/2
    setView({start:center-half,end:center+half})
  }
  async function focusSelection(keys:readonly NativeScalarKeySelection[]){await tick();if(alive){const target=[...(root?.querySelectorAll<HTMLButtonElement>('[data-native-key]')??[])].find(button=>keys.some(key=>button.dataset.nativeKey===JSON.stringify(key)));target?.focus({preventScroll:true})}}
  function selectRange(){
    if(blocked)return
    try{const hits=scalarKeysInTimeRange(channels,selectionFrom===undefined?first:selectionFrom*1e6,selectionTo===undefined?last:selectionTo*1e6);cancel();selection=combineScalarKeySelection(selection,hits,selectionMode);error='';void focusSelection(selection)}catch(cause){error=cause instanceof Error?cause.message:String(cause)}
  }
  function rangeDown(event:PointerEvent){
    if(blocked||drag||marquee||event.button!==0)return
    const element=event.currentTarget as HTMLButtonElement,rect=element.parentElement!.getBoundingClientRect(),wrapper=lanes!.getBoundingClientRect()
    if(!rect.width||!rect.height)return
    const held=context();if(!held)return
    const points:Marquee['points']=[],rows:Marquee['rows']=[];let top=Infinity,bottom=-Infinity
    for(const row of lanes!.querySelectorAll<HTMLElement>('[data-native-channel]')){
      const channel=channels.find(channel=>channel.id===row.dataset.nativeChannel);if(!channel||channel.locked)continue
      const box=row.getBoundingClientRect();if(!box.width||!box.height)continue
      rows.push({element:row,rect:box});top=Math.min(top,box.top);bottom=Math.max(bottom,box.bottom)
      const centers=new Map([...row.querySelectorAll<HTMLButtonElement>('[data-native-key]')].map(button=>{const bounds=button.getBoundingClientRect();return [button.dataset.nativeKey,bounds.height>0?bounds.top+bounds.height/2:box.top+box.height/2] as const}))
      for(const key of channel.value.track.keyframes){const time=keyTime(channel,key.tick);points.push({key:address(channel,key.tick),time,left:box.left,width:box.width,x:box.left+(time-first)/(last-first)*box.width,y:centers.get(JSON.stringify(address(channel,key.tick)))??centers.values().next().value??box.top+box.height/2})}
    }
    if(!points.length){cancel();return}
    event.preventDefault();element.setPointerCapture?.(event.pointerId)
    const mode:ScalarSelectionMode=event.altKey?'subtract':event.ctrlKey||event.metaKey?'toggle':event.shiftKey?'add':selectionMode
    marquee={context:held,pointer:event.pointerId,element,rect,wrapper,x:event.clientX,y:event.clientY,toX:event.clientX,toY:event.clientY,points,rows,anchorUs:first+(event.clientX-rect.left)/rect.width*(last-first),clientX:event.clientX,mode,moved:false,result:[...selection],top,bottom}
  }
  function rangeValid(held:Marquee){return owns(held.context)&&!nestedHistory.busy&&held.rows.every(row=>{const rect=row.element.getBoundingClientRect();return row.element.isConnected&&(['left','top','width','height'] as const).every(key=>Math.abs(rect[key]-row.rect[key])<.5)})}
  function publishRange(held:Marquee){
    held.x=held.rect.left+(held.anchorUs-first)/(last-first)*held.rect.width
    const points=held.points.map(point=>({...point,x:point.left+(point.time-first)/(last-first)*point.width}))
    held.result=combineScalarKeySelection(selection,scalarKeysInBox(points,held.x,held.y,held.toX,held.toY),held.mode);marquee={...held}
  }
  function startRangePan(){
    const held=marquee
    if(panFrame!==undefined||!held?.moved||!scalarEdgePanSpeed(held.clientX,held.rect.left,held.rect.width))return
    panStamp=performance.now();panFrame=requestAnimationFrame(autoRangePan)
  }
  function autoRangePan(stamp:number){
    panFrame=undefined
    const held=marquee;if(!held)return
    if(!rangeValid(held)){cancel();return}
    const speed=scalarEdgePanSpeed(held.clientX,held.rect.left,held.rect.width),elapsed=Math.max(0,Math.min(50,stamp-panStamp));panStamp=stamp
    if(!speed)return
    const next=panScalarTimeView(visible,extent,speed*(last-first)*elapsed/1000)
    if(next.start===first&&next.end===last)return
    view=next;held.context.view=JSON.stringify([next.start,next.end]);publishRange(held)
    panFrame=requestAnimationFrame(autoRangePan)
  }
  function rangeMove(event:PointerEvent){
    const held=marquee;if(!held||held.pointer!==event.pointerId)return
    if(!rangeValid(held)){cancel();return}
    held.moved ||= Math.hypot(event.clientX-(held.rect.left+(held.anchorUs-first)/(last-first)*held.rect.width),event.clientY-held.y)>=3
    if(!held.moved)return
    event.preventDefault();held.clientX=event.clientX;held.toX=Math.max(held.rect.left,Math.min(held.rect.right,event.clientX));held.toY=Math.max(held.top,Math.min(held.bottom,event.clientY))
    publishRange(held)
    if(scalarEdgePanSpeed(held.clientX,held.rect.left,held.rect.width))startRangePan();else stopPan()
  }
  function rangeUp(event:PointerEvent){
    const held=marquee;if(!held||held.pointer!==event.pointerId)return
    rangeMove(event)
    if(!rangeValid(held)){cancel();return}
    const moved=held.moved,next=held.result;cancel()
    if(moved){selection=next;void focusSelection(next)}
    else{if(held.mode==='replace')selection=[];nestedPlayback.playing=false;nestedPlayback.epoch++;nestedComposition.timeUs=Math.max(0,Math.round(first+(Math.max(held.rect.left,Math.min(held.rect.right,event.clientX))-held.rect.left)/held.rect.width*(last-first)))}
  }
  function rangeClick(event:MouseEvent,channel:NativeScalarChannel){if(!event.detail&&!blocked){cancel();selection=combineScalarKeySelection(selection,scalarKeysInTimeRange([channel],first,last),selectionMode);void focusSelection(selection)}}
  function selectAll(){if(!blocked)selection=channels.filter(channel=>!channel.locked).flatMap(channel=>channel.value.track.keyframes.map(key=>address(channel,key.tick)))}
  async function action(kind:'delete'|'preset',preset?:string){
    if(blocked||!selection.length)return
    const held=context();if(!held)return
    try{const operations=nativeScalarSelectionOperations(held.base,scope,selection,kind,preset);await commit(held,operations,kind==='delete'?[]:[...selection])}catch(cause){cancel();error=cause instanceof Error?cause.message:String(cause)}
  }
  function add(channel:NativeScalarChannel){
    if(blocked||channel.locked)return
    const local=nestedComposition.timeUs-channel.start
    if(local<0||local>channel.duration){error='Курсор должен находиться внутри клипа';return}
    const held=context();if(!held)return
    const keyTick=keyframeAuthoredTick(channel.value.track,local)
    void commit(held,[{op:'upsert_keyframe',...nativeScalarAddress(channel),tick:keyTick,value:sampleAnimatableValue(channel.value,local)}],[address(channel,keyTick)])
  }
  function previewScale(){
    if(blocked||selection.length<2)return
    const held=context();if(!held)return
    try{
      const times=selection.map(item=>{const channel=channels.find(channel=>channel.id===item.channelId)!;return channel.start+keyframeLocalTime(channel.value.track,item.tick)})
      const anchor=Math.round(scaleAnchor==='cursor'?nestedComposition.timeUs:scaleAnchor==='last'?Math.max(...times):Math.min(...times))
      const plan=planNativeScalarKeyScale(held.base,held.scope,selection,anchor,scaleFactor)
      if(!plan.operations.length){cancel();error='Масштаб не изменяет положение ключей';return}
      scalePreview={context:held,plan,parameters:scaleParameters,anchor}
      nestedEffectPreview.value={owner,gesture:held,base:held.base,project:plan.project,timeUs:held.time,compositionId:held.scope,token:held.token}
    }catch(cause){cancel();error=cause instanceof Error?cause.message:String(cause)}
  }
  function applyScale(){const preview=scalePreview;if(!preview||preview.parameters!==scaleParameters){cancel();return}void commit(preview.context,preview.plan.operations,preview.plan.selection)}
  function shown(channel:NativeScalarChannel,keyTick:number){
    const original=channel.start+keyframeLocalTime(channel.value.track,keyTick)
    const plan=drag?.plan??scalePreview?.plan
    if(!plan||!selected(address(channel,keyTick)))return original
    const originalTicks=selection.filter(item=>item.channelId===channel.id).map(item=>item.tick).sort((a,b)=>a-b)
    const destination=plan.selection.filter(item=>item.channelId===channel.id)[originalTicks.indexOf(keyTick)]
    return destination?channel.start+keyframeLocalTime(channel.value.track,destination.tick):original
  }
</script>
<svelte:window onkeydown={event=>{
  if(event.key==='Escape'&&work)cancel()
  else if(root?.contains(event.target as Node)&&!(event.target instanceof HTMLInputElement)&&!(event.target instanceof HTMLSelectElement)&&!(event.target instanceof HTMLTextAreaElement))clipboardShortcut(event)
}} onresize={()=>{if(marquee||drag)cancel()}} />
{#if project&&project.compositions[scope]?.content.tracks.some(track=>track.clips.length)}
  <section bind:this={root} aria-label="Нативный таймлайн скалярных ключей">
    <h4>Дорожки ключей</h4>
    <NestedScalarClipboard bind:this={clipboardControls} {selection} onselection={acceptEditedSelection} {disabled} onactivate={cancel} />
    <NestedScalarGraph onviewportpan={panGraphView} {selection} onselection={keys=>selection=keys} oncommitselection={acceptEditedSelection} onviewportwheel={(event,fraction,width)=>wheelView(event,{fraction,width})} rangeStart={first} rangeEnd={last} {disabled} onactivate={cancel} />
    <div class="tools">
      <button type="button" class="btn ghost sm" disabled={blocked} onclick={selectAll}>Выбрать все ключи</button>
      <button type="button" class="btn ghost sm" disabled={blocked||!selection.length} onclick={()=>selection=[]}>Снять выделение ключей</button>
      <button type="button" class="btn ghost sm" disabled={blocked||!selection.length} onclick={()=>void action('delete')}>Удалить выбранные ключи</button>
      <label>Переход<select aria-label="Переход выбранных нативных ключей" disabled={blocked||!selection.length} value="" onchange={event=>{void action('preset',event.currentTarget.value);event.currentTarget.value=''}}><option value="" disabled>Выберите переход</option>{#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}</select></label>
      <span aria-live="polite">Выбрано: {displaySelection.length}</span>
    </div>
    <fieldset disabled={blocked}>
      <legend>Масштаб времени выделенных ключей</legend>
      <label>Коэффициент<input aria-label="Масштаб времени нативных ключей" type="number" min="0.001" step="0.1" bind:value={scaleFactor} /></label>
      <label>Опора<select aria-label="Опора масштаба нативных ключей" value={scaleAnchor} onchange={event=>scaleAnchor=event.currentTarget.value as 'first'|'last'|'cursor'}><option value="first">Первый выбранный ключ</option><option value="last">Последний выбранный ключ</option><option value="cursor">Курсор</option></select></label>
      <button type="button" class="btn ghost sm" disabled={selection.length<2} onclick={previewScale}>Предпросмотр масштаба ключей</button>
      <button type="button" class="btn ghost sm" disabled={!scalePreview} onclick={applyScale}>Применить масштаб ключей</button>
    </fieldset>
    {#if scalePreview}<p role="status">Масштаб: {scalePreview.plan.factor.toFixed(6)} · опора {(scalePreview.anchor/1e6).toFixed(3)} с</p><button type="button" class="btn ghost sm" onclick={cancel}>Отменить масштаб ключей</button>{/if}
    <p>Shift или Ctrl/⌘ — несколько ключей. Перетаскивание и стрелки сдвигают выделение по кадрам; Shift со стрелкой — десять кадров. Escape отменяет жест.</p>
    <fieldset disabled={blocked}>
      <legend>Видимая область ключей</legend>
      <button type="button" class="btn ghost sm" onclick={()=>zoomView(2)}>Приблизить ключи по курсору</button>
      <button type="button" class="btn ghost sm" onclick={()=>zoomView(0.5)}>Отдалить ключи по курсору</button>
      <button type="button" class="btn ghost sm" onclick={()=>panView(-1)}>Область ключей раньше</button>
      <button type="button" class="btn ghost sm" onclick={()=>panView(1)}>Область ключей позже</button>
      <button type="button" class="btn ghost sm" onclick={()=>setView(null)}>Показать все ключи</button>
      <button type="button" class="btn ghost sm" disabled={!selection.length} onclick={fitSelection}>Показать выделенные ключи</button>
      <label>От, с<input aria-label="Начало видимой области ключей" type="number" step="any" placeholder={String(first/1e6)} bind:value={viewFrom} /></label>
      <label>До, с<input aria-label="Конец видимой области ключей" type="number" step="any" placeholder={String(last/1e6)} bind:value={viewTo} /></label>
      <button type="button" class="btn ghost sm" onclick={()=>setView({start:viewFrom===undefined?first:viewFrom*1e6,end:viewTo===undefined?last:viewTo*1e6})}>Применить область ключей</button>
    </fieldset>
    <fieldset disabled={blocked}>
      <legend>Выделение по времени</legend>
      <label>От, с<input aria-label="Начало выделения ключей" type="number" step="any" placeholder={String(first/1e6)} bind:value={selectionFrom} /></label>
      <label>До, с<input aria-label="Конец выделения ключей" type="number" step="any" placeholder={String(last/1e6)} bind:value={selectionTo} /></label>
      <label>Режим<select aria-label="Режим выделения нативных ключей" value={selectionMode} onchange={event=>selectionMode=event.currentTarget.value as ScalarSelectionMode}><option value="replace">Заменить</option><option value="add">Добавить</option><option value="toggle">Переключить</option><option value="subtract">Вычесть</option></select></label>
      <button type="button" class="btn ghost sm" onclick={selectRange}>Выделить ключи по времени</button>
    </fieldset>
    <p>Ctrl/⌘ с колесом — масштаб вокруг указателя; Shift с колесом — сдвиг области. Перетаскивание ключей или рамки у края дорожки прокручивает время. Пустое место дорожек — рамка выделения. Shift добавляет, Ctrl/⌘ переключает, Alt вычитает ключи.</p>
    <div class="range" aria-label="Видимый диапазон ключей">{(first/1e6).toFixed(3)}–{(last/1e6).toFixed(3)} с</div>
    <div class="lanes" bind:this={lanes}>
    {#each channels as channel (channel.id)}
      <div class="lane">
        <span class="label">{channel.label}{channel.locked?' · 🔒':''}</span>
        <div class="ruler" role="group" aria-label={channel.label} data-native-channel={channel.id} onwheel={wheelView}>
          <button type="button" class="range-surface" disabled={blocked||channel.locked} aria-label={`Выделить диапазон ${nativeScalarTargetLabel(channel)}`} onpointerdown={rangeDown} onpointermove={rangeMove} onpointerup={rangeUp} onpointercancel={event=>{if(marquee?.pointer===event.pointerId)cancel()}} onlostpointercapture={event=>{if(marquee?.element===event.currentTarget)cancel()}} onclick={event=>rangeClick(event,channel)}></button>
          <span class="clip" style:left={`${100*(Math.max(first,channel.start)-first)/(last-first)}%`} style:width={`${100*Math.max(0,Math.min(last,channel.start+channel.duration)-Math.max(first,channel.start))/(last-first)}%`}></span>
          {#if nestedComposition.timeUs>=first&&nestedComposition.timeUs<=last}<span class="cursor" style:left={`${100*(nestedComposition.timeUs-first)/(last-first)}%`}></span>{/if}
          {#each channel.value.track.keyframes.filter(key=>keyTime(channel,key.tick)>=first&&keyTime(channel,key.tick)<=last||!!drag&&selected(address(channel,key.tick))) as key (key.tick)}
            <button type="button" class="mark" class:outside={shown(channel,key.tick)<first||shown(channel,key.tick)>last} class:selected={selected(address(channel,key.tick))} disabled={blocked||channel.locked} data-native-key={JSON.stringify(address(channel,key.tick))} style:left={`${100*(shown(channel,key.tick)-first)/(last-first)}%`} aria-pressed={selected(address(channel,key.tick))} aria-label={`${nativeScalarTargetLabel(channel)} ключ ${key.tick}`} title={`${(shown(channel,key.tick)/1e6).toFixed(3)} с · ${key.value}`} onpointerdown={event=>down(event,channel,key.tick)} onpointermove={move} onpointerup={up} onpointercancel={event=>{if(drag?.pointer===event.pointerId)cancel()}} onlostpointercapture={event=>{if(drag?.svg===event.currentTarget)cancel()}} onclick={event=>click(event,channel,key.tick)} onkeydown={event=>nudge(event,channel,key.tick)}>◆</button>
          {/each}
        </div>
        <button type="button" class="btn ghost sm" aria-label={`Добавить ключ ${nativeScalarTargetLabel(channel)} по курсору`} disabled={blocked||channel.locked} onclick={()=>add(channel)}>+ Ключ</button>
      </div>
    {/each}
    {#if marquee?.moved}<div class="marquee" aria-label="Рамка выделения ключей" style:left={`${Math.max(marquee.rect.left,Math.min(marquee.x,marquee.toX))-marquee.wrapper.left}px`} style:top={`${Math.min(marquee.y,marquee.toY)-marquee.wrapper.top}px`} style:width={`${Math.min(marquee.rect.right,Math.max(marquee.x,marquee.toX))-Math.max(marquee.rect.left,Math.min(marquee.x,marquee.toX))}px`} style:height={`${Math.abs(marquee.toY-marquee.y)}px`}></div>{/if}
    </div>
    {#if drag?.plan}<p role="status">Сдвиг: {(drag.plan.deltaUs/1e6).toFixed(3)} с</p>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </section>
{/if}
<style>
  fieldset{border:0;padding:.5rem 0;display:flex;gap:.5rem;flex-wrap:wrap}input{width:6rem}
  .lanes{position:relative}.range-surface{position:absolute;inset:0;width:100%;height:100%;border:0;background:transparent;cursor:crosshair;touch-action:none}.marquee{position:absolute;border:1px solid var(--accent,#6af);background:rgba(80,160,255,.15);pointer-events:none;z-index:3}.mark.outside{visibility:hidden}
  .tools,.lane{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap}.label{width:13rem;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.ruler{position:relative;flex:1;min-width:12rem;height:2rem;margin:0 .75rem;border-bottom:1px solid var(--border,#555)}.mark{position:absolute;z-index:2;top:0;transform:translateX(-50%);padding:.25rem;border:0;background:transparent;color:inherit;touch-action:none;cursor:ew-resize}.mark.selected{color:var(--accent,#6af);outline:1px solid currentColor}.mark:disabled{opacity:.4}.clip{position:absolute;top:.15rem;height:1.5rem;background:var(--border,#555);opacity:.3;pointer-events:none}.cursor{position:absolute;top:0;height:100%;border-left:1px solid var(--accent,#6af);pointer-events:none}.range,p{font-size:.8rem}h4{margin:.5rem 0}select{max-width:12rem}
</style>
