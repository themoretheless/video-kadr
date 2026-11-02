<script lang="ts">
  import { nestedPathOverlay, type PathOverlaySelection } from '$lib/state/nestedPathOverlay.svelte'
  import { nestedEffectPreview, clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
  import { nestedComposition, nestedHistory, nestedAgentGeneration, editNestedMotionPath } from '$lib/state/nestedComposition.svelte'
  import { authState } from '$lib/state/auth.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  import { applyNativePathOperation } from '$lib/agent/nativePath'
  import { nativeClipStart } from '$lib/composition/nativeClipMove'
  import { nativeClipEnd } from '$lib/composition/nativeClipInsert'
  import type { WireCompositionTrack } from '$lib/composition/types'
  import type { NativeNestedProject } from '$lib/api'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Part = 'point' | 'in_handle' | 'out_handle'
  type Edit = { operation:'edit'; index:number; part:Part; value:{x:number;y:number} }
  type Context = { selection:PathOverlaySelection; identity:object; time:number }
  type Gesture = Context & { svg:SVGSVGElement; pointerId:number; rect:DOMRect; x:number; y:number; index:number; part:Part; origin:{x:number;y:number}; original:NativeNestedProject; controller:AbortController; active:boolean; running:boolean; next?:Edit }
  const owner = Symbol('spatial path preview')
  let alive = true, pending = $state(false), drag = $state.raw<Gesture|null>(null)
  let draft = $state.raw<Edit|null>(null), preview = $state.raw<NativeNestedProject|null>(null)
  const project = $derived(nestedComposition.document)
  const selection = $derived(nestedPathOverlay.value)
  const scope = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const view = $derived(selection && project && selection.projectId === nestedComposition.projectId && selection.generation === nestedAgentGeneration() && selection.compositionId === scope && selection.token === authState.token ? selection : null)
  const track = $derived(project?.compositions[scope]?.content.tracks.find(item => item.clips.some(clip => clip.id === view?.clipId)))
  const clip = $derived(track?.clips.find(item => item.id === view?.clipId))
  const position = $derived(clip && 'transform' in clip && clip.transform.position?.mode === 'path' ? clip.transform.position : undefined)
  const canvas = $derived(project?.compositions[scope]?.content.canvas)
  const visible = $derived(view && position && clip && nestedComposition.timeUs >= nativeClipStart(clip) && nestedComposition.timeUs <= nativeClipEnd(clip))
  const blocked = $derived(disabled || pending || nestedHistory.busy || !!agentSession.lock || !!track?.locked)
  const field = (part:Part) => part === 'point' ? 'point' : part === 'in_handle' ? 'inHandle' : 'outHandle'
  const previewClip = $derived(preview?.compositions[scope]?.content.tracks.flatMap<WireCompositionTrack['clips'][number]>(item=>item.clips).find(item=>item.id===view?.clipId))
  const shown = $derived(previewClip && 'transform' in previewClip && previewClip.transform.position?.mode === 'path' ? previewClip.transform.position.path.nodes : position?.path.nodes ?? [])
  const nodes = $derived(shown.map((node,index)=>{const change=draft;return change&&change.index===index?{...node,[field(change.part)]:change.value}:node}))
  const pathData = $derived(nodes.map((node,index)=>{
    if(!index)return `M ${node.point.x} ${node.point.y}`
    const previous=nodes[index-1]!
    return previous.segment==='line'?`L ${node.point.x} ${node.point.y}`:`C ${previous.point.x+previous.outHandle.x} ${previous.point.y+previous.outHandle.y} ${node.point.x+node.inHandle.x} ${node.point.y+node.inHandle.y} ${node.point.x} ${node.point.y}`
  }).join(' '))
  function owns(context:Context){return alive && nestedPathOverlay.value===context.selection && project===context.selection.base && scope===context.selection.compositionId && authState.token===context.selection.token && nestedAgentGeneration()===context.selection.generation}
  function active(gesture:Gesture){return drag===gesture && owns(gesture) && !blocked && nestedComposition.timeUs===gesture.time}
  function publish(context:Context,candidate:NativeNestedProject){
    if(!owns(context)||nestedComposition.timeUs!==context.time)return
    preview=candidate
    nestedEffectPreview.value={owner,gesture:context.identity,base:context.selection.base,project:candidate,timeUs:context.time,compositionId:context.selection.compositionId,token:context.selection.token}
  }
  async function pump(gesture:Gesture){
    if(gesture.running)return
    gesture.running=true
    try{
      while(active(gesture)&&gesture.next){
        const edit=gesture.next;gesture.next=undefined
        const candidate=await applyNativePathOperation(gesture.original,gesture.selection.compositionId,{op:'edit_motion_path',clipId:gesture.selection.clipId,edit},gesture.controller.signal)
        if(active(gesture))publish(gesture,candidate)
      }
    }catch(error){if(active(gesture)&&!gesture.controller.signal.aborted)nestedHistory.error=error instanceof Error?error.message:String(error)}
    finally{gesture.running=false}
  }
  function cancel(){
    const gesture=drag;drag=null;draft=null;preview=null
    if(gesture){gesture.controller.abort();clearEffectPreview(owner,gesture.identity)}
  }
  function begin(event:PointerEvent,index:number,part:Part='point'){
    if(drag||event.button!==0||blocked||!visible||!view||view.base!==project||!canvas||!position)return
    const svg=(event.currentTarget as SVGElement).ownerSVGElement!,rect=svg.getBoundingClientRect()
    if(!rect.width||!rect.height)return
    event.preventDefault();event.stopPropagation();svg.setPointerCapture?.(event.pointerId)
    const point=position.path.nodes[index]![field(part)]
    drag={selection:view,identity:{},time:nestedComposition.timeUs,svg,pointerId:event.pointerId,rect,x:event.clientX,y:event.clientY,index,part,origin:{...point},original:JSON.parse(JSON.stringify(project)) as NativeNestedProject,controller:new AbortController(),active:false,running:false}
  }
  function move(event:PointerEvent){
    const gesture=drag
    if(!gesture||gesture.pointerId!==event.pointerId||!canvas)return
    if(!active(gesture)){cancel();return}
    gesture.active ||= Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y)>=3
    if(!gesture.active)return
    const value={x:gesture.origin.x+(event.clientX-gesture.x)*canvas.width/gesture.rect.width,y:gesture.origin.y+(event.clientY-gesture.y)*canvas.height/gesture.rect.height}
    if(draft?.index===gesture.index&&draft.part===gesture.part&&draft.value.x===value.x&&draft.value.y===value.y)return
    draft={operation:'edit',index:gesture.index,part:gesture.part,value}
    gesture.next=draft;void pump(gesture)
  }
  async function commit(context:Context,operation:Record<string,unknown>){
    if(!owns(context)||blocked)return
    pending=true
    try{
      await editNestedMotionPath(context.selection.compositionId,{...operation,clipId:context.selection.clipId},()=>{if(!owns(context))throw new Error('Контекст редактирования пути изменился')},candidate=>publish(context,candidate))
    }finally{pending=false;preview=null;clearEffectPreview(owner,context.identity)}
  }
  function finish(event?:PointerEvent,abort=false){
    const gesture=drag
    if(!gesture||event&&gesture.pointerId!==event.pointerId)return
    if(event&&!abort)move(event)
    const edit=draft;drag=null;draft=null;gesture.controller.abort()
    if(gesture.svg.hasPointerCapture?.(gesture.pointerId))gesture.svg.releasePointerCapture(gesture.pointerId)
    if(abort||!edit||!owns(gesture)||blocked){preview=null;clearEffectPreview(owner,gesture.identity);return}
    void commit(gesture,{op:'edit_motion_path',edit})
  }
  function keyboard(event:KeyboardEvent,index:number,part:Part='point'){
    if(blocked||!view||!position||!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key))return
    event.preventDefault();event.stopPropagation()
    const point=position.path.nodes[index]![field(part)],step=event.shiftKey?10:1
    void commit({selection:view,identity:{},time:nestedComposition.timeUs},{op:'edit_motion_path',edit:{operation:'edit',index,part,value:{x:point.x+(event.key==='ArrowRight'?step:event.key==='ArrowLeft'?-step:0),y:point.y+(event.key==='ArrowDown'?step:event.key==='ArrowUp'?-step:0)}}})
  }
  function insert(event:MouseEvent){
    if(blocked||!view||!canvas||event.target instanceof SVGElement&&event.target.localName==='circle')return
    const rect=(event.currentTarget as SVGSVGElement).getBoundingClientRect();if(!rect.width||!rect.height)return
    event.preventDefault();event.stopPropagation()
    void commit({selection:view,identity:{},time:nestedComposition.timeUs},{op:'insert_path_node_near',nodeId:crypto.randomUUID(),point:{x:(event.clientX-rect.left)*canvas.width/rect.width-canvas.width/2,y:(event.clientY-rect.top)*canvas.height/rect.height-canvas.height/2},maxDistance:8*Math.max(canvas.width/rect.width,canvas.height/rect.height)})
  }
  $effect(()=>{if(drag&&!active(drag))cancel()})
  $effect(()=>{
    const current=nestedEffectPreview.value
    if(current?.owner===owner&&(!view||project!==current.base||nestedComposition.timeUs!==current.timeUs||disabled||!!agentSession.lock||!!track?.locked))clearEffectPreview(owner)
    if(!view||view.base!==project)preview=null
  })
  $effect(()=>()=>{alive=false;cancel();clearEffectPreview(owner)})
</script>
<svelte:window onkeydown={event=>{if(drag&&event.key==='Escape'){event.preventDefault();event.stopPropagation();cancel()}}} />
{#if visible && canvas}
  <svg aria-label="Траектория на кадре" role="group" viewBox={`0 0 ${canvas.width} ${canvas.height}`} preserveAspectRatio="none" onpointermove={move} onpointerup={event=>finish(event)} onpointercancel={event=>finish(event,true)} onlostpointercapture={()=>finish(undefined,true)} ondblclick={insert}>
    <g transform={`translate(${canvas.width/2} ${canvas.height/2})`}>
      <path d={pathData} fill="none" stroke="var(--accent,#60a5fa)" stroke-width="2" vector-effect="non-scaling-stroke" style="pointer-events:stroke" />
      {#each nodes as node,index (node.id)}
        {#each ['in_handle','out_handle'] as part (part)}
          {@const handle=node[field(part as Part)]}
          <line x1={node.point.x} y1={node.point.y} x2={node.point.x+handle.x} y2={node.point.y+handle.y} stroke="var(--accent,#60a5fa)" opacity="0.5" vector-effect="non-scaling-stroke" />
          <circle role="button" tabindex={blocked?-1:0} aria-disabled={blocked} aria-label={`Ручка пути ${index+1} ${part}`} cx={node.point.x+handle.x} cy={node.point.y+handle.y} r="3" onpointerdown={event=>begin(event,index,part as Part)} onkeydown={event=>keyboard(event,index,part as Part)} />
        {/each}
      {/each}
      {#each nodes as node,index (node.id)}<circle role="button" tabindex={blocked?-1:0} aria-disabled={blocked} aria-label={`Узел пути на кадре ${index+1}`} cx={node.point.x} cy={node.point.y} r="4" onpointerdown={event=>begin(event,index)} onkeydown={event=>keyboard(event,index)} />{/each}
    </g>
  </svg>
{/if}
<style>svg{position:absolute;inset:0;width:100%;height:100%;z-index:5;pointer-events:none;overflow:visible;touch-action:none}circle{pointer-events:auto;fill:var(--panel-2,white);stroke:var(--accent,#60a5fa);vector-effect:non-scaling-stroke}</style>
