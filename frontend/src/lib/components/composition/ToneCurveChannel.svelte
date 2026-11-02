<script lang="ts">
  import CurvePointGraph,{type CurveGesture,type CurvePointDraft}from'./CurvePointGraph.svelte'
  import{addCurvePoint,moveCurvePoint,removeCurvePoint}from'$lib/composition/concatCurveAuthoring'
  import { sampleToneCurve } from '$lib/api'
  type Points=readonly(readonly[number,number])[]
  type Reference={points:Points;tint:string}
  let{points,label,disabled=false,periodic=false,sampleCurve=sampleToneCurve,maxPoints=256,interactive=false,scope,clock=0,session='',context='',tint='#a8c8ff',references=[],onpreview,onchange}:{points:Points;label:string;disabled?:boolean;periodic?:boolean;sampleCurve?:typeof sampleToneCurve;maxPoints?:number;interactive?:boolean;scope?:unknown;clock?:number;session?:string;context?:string;tint?:string;references?:readonly Reference[];onpreview?:(draft:CurvePointDraft|null,gesture:object)=>void;onchange:(points:Points,meta?:CurveGesture)=>void|Promise<void>}=$props()
  let preview=$state.raw<CurvePointDraft|undefined>()
  let held=$state.raw<{points:Points;meta:CurveGesture}|undefined>()
  let ghosts=$state<readonly{path:string;tint:string}[]>([]),referenceError=$state('')
  const curvePath=(values:readonly number[])=>values.map((value,i)=>`${i?'L':'M'}${i/255*200},${200-value*200}`).join(' ')
  $effect(()=>{const base=scope,time=clock,account=session,target=context;if(held&&(held.meta.scope!==base||held.meta.clock!==time||held.meta.session!==account||held.meta.context!==target))held=undefined})
  $effect(()=>{const refs=references,sampler=sampleCurve;const abort=new AbortController();let active=true;ghosts=[];referenceError='';if(refs.length)void Promise.all(refs.map(async ref=>({path:curvePath(await sampler(ref.points,abort.signal,false)),tint:ref.tint}))).then(paths=>{if(active)ghosts=paths}).catch(reason=>{if(active&&!abort.signal.aborted)referenceError=reason instanceof Error?reason.message:String(reason)});return()=>{active=false;abort.abort()}})
  function previewPoints(draft:CurvePointDraft|null,gesture:object){preview=draft??undefined;onpreview?.(draft,gesture)}
  async function apply(next:Points,meta?:CurveGesture){if(!interactive){await onchange(next);return}const current=meta??{scope,clock,session,context,gesture:{}};const hold={points:next,meta:current};held=hold;try{await onchange(next,current)}catch(reason){error=reason instanceof Error?reason.message:String(reason)}finally{if(held===hold)held=undefined}}
  let path = $state('')
  let error = $state('')
  $effect(() => {
    const current = preview?.points??held?.points??points
    const abort = new AbortController()
    let active = true
    error = ''; path = ''
    void sampleCurve(current, abort.signal, periodic).then((values) => {
      if(active)path=curvePath(values)
    }).catch((failure) => { if (active && !abort.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) })
    return () => { active = false; abort.abort() }
  })
  function change(index: number, axis: 0 | 1, value: number): void {
    if(interactive){try{const p=points[index]!;void apply(moveCurvePoint(points,index,axis===0?value:p[0],axis===1?value:p[1]))}catch(reason){error=reason instanceof Error?reason.message:String(reason)}}else void apply(points.map((point,i)=>i===index?axis===0?[value,point[1]]:[point[0],value]:point))
  }
  function add(): void {
    if(points.length>=maxPoints)return
    if(interactive&&points.length<2){void apply(addCurvePoint(points,0.5,0.5));return}
    if (!points.length) { void apply(periodic ? [[0, 0.5], [0.5, 0.5]] : [[0, 0], [0.5, 0.5], [1, 1]]); return }
    const positions = [...new Set([0, ...points.map((point) => point[0]), 1])].sort((a, b) => a - b)
    let start = 0, end = 1, width = -1
    for (let i = 1; i < positions.length; i += 1) {
      if (positions[i]! - positions[i - 1]! > width) { start = positions[i - 1]!; end = positions[i]!; width = end - start }
    }
    const x = (start + end) / 2
    void apply(interactive?addCurvePoint(points,x,x):[...points,[x,periodic?0.5:x]])
  }
</script>
<div class="curve-channel">
  {#if interactive}
   <CurvePointGraph {points} shown={preview?.points??held?.points} {path} {label} {disabled} {scope} {clock} {session} {context} {tint} {ghosts} onchange={(next,meta)=>void apply(next,meta)} onpreview={previewPoints}/>
  {:else}
  <svg viewBox="0 0 200 200" role="img" aria-label={`Кривая ${label}`}>
    <path d={periodic ? "M0 100L200 100" : "M0 200L200 0"} class="identity" />
    <path d={path} class="curve" />
    {#each points as point}<circle cx={point[0] * 200} cy={200 - point[1] * 200} r="3" />{/each}
  </svg>
  {/if}
  <div class="points">
    {#if error||referenceError}<p role="alert">{error||referenceError}</p>{/if}
    {#each points as point, index}
      <div class="point">
        <label>Вход <input aria-label={`${label} точка ${index + 1} вход`} type="number" min="0" max="1" step="0.01" value={point[0]} {disabled} onchange={(event) => change(index, 0, Number(event.currentTarget.value))} /></label>
        <label>Выход <input aria-label={`${label} точка ${index + 1} выход`} type="number" min="0" max="1" step="0.01" value={point[1]} {disabled} onchange={(event) => change(index, 1, Number(event.currentTarget.value))} /></label>
        <button class="btn ghost sm" {disabled} onclick={()=>void apply(interactive?removeCurvePoint(points,index):points.filter((_,i)=>i!==index))}>Удалить точку {index + 1}</button>
      </div>
    {/each}
    <button class="btn ghost sm" disabled={disabled || points.length >= maxPoints} onclick={add}>Добавить точку</button>
    <button class="btn ghost sm" {disabled} onclick={()=>void apply([])}>Сбросить канал</button>
  </div>
</div>
<style>
  .curve-channel { display: flex; gap: 0.75rem; flex-wrap: wrap; }
  svg { width: 200px; height: 200px; border: 1px solid var(--border, #444); flex-shrink: 0; }
  path { fill: none; }
  .identity { stroke: #555; }
  .curve { stroke: #a8c8ff; stroke-width: 1.5; }
  circle { fill: #a8c8ff; }
  .point { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  input { width: 4rem; }
</style>
