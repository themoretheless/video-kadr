<script lang="ts">
  import { previewPositionConversion, type NativeNestedProject, type NativePositionConversionResult } from '$lib/api'
  import { nestedComposition, nestedHistory, nestedAgentGeneration, commitNestedPositionConversion } from '$lib/state/nestedComposition.svelte'
  import { nestedEffectPreview, clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
  import { nativePositionConversionCandidate } from '$lib/composition/nativePositionConversion'
  import { packPositionConversion } from '$lib/composition/positionConversion'
  import { nativeClipStart } from '$lib/composition/nativeClipMove'
  import { nativeClipEnd } from '$lib/composition/nativeClipInsert'
  import { authState } from '$lib/state/auth.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  let { compositionId, clipId, disabled=false }:{compositionId:string;clipId:string;disabled?:boolean}=$props()
  let opened=$state(false),startFrame=$state(0),endFrame=$state<number|undefined>(),tolerance=$state(0.5),interval=$state(1),mode=$state<'editable'|'bake'>('editable')
  let pending=$state(false),applying=$state(false),message=$state('')
  const project=$derived(nestedComposition.document)
  const node=$derived(project?.compositions[compositionId])
  const track=$derived(node?.content.tracks.find(row=>row.clips.some(clip=>clip.id===clipId)))
  const clip=$derived(track?.clips.find(clip=>clip.id===clipId))
  const transform=$derived(clip&&'transform' in clip?clip.transform:undefined)
  const position=$derived(transform?.position??(transform?{mode:'xy' as const,x:transform.x,y:transform.y}:null))
  const parameters=$derived(JSON.stringify([startFrame,endFrame,tolerance,interval,mode]))
  const unavailable=$derived(disabled||!!track?.locked||!!agentSession.lock||!authState.token||!transform)
  const blocked=$derived(unavailable||nestedHistory.busy||applying)
  const owner=Symbol('position conversion preview')
  type Context={base:NativeNestedProject;signature:string;scope:string;clipId:string;projectId:string|null;generation:number;token:string;time:number;parameters:string;controller:AbortController;prefix:string}
  let work=$state.raw<Context|null>(null),result=$state.raw<NativePositionConversionResult|null>(null),alive=true
  function owns(context:Context){return alive&&work===context&&project===context.base&&JSON.stringify(project)===context.signature&&compositionId===context.scope&&clipId===context.clipId&&(nestedComposition.activeCompositionId??project?.root)===context.scope&&nestedComposition.projectId===context.projectId&&nestedAgentGeneration()===context.generation&&authState.token===context.token&&nestedComposition.timeUs===context.time&&parameters===context.parameters&&!unavailable&&(!nestedEffectPreview.value||nestedEffectPreview.value.owner===owner)&&!context.controller.signal.aborted}
  function reset(){work?.controller.abort();work=null;result=null;pending=false;clearEffectPreview(owner)}
  $effect(()=>{const context=work;if(context&&(!opened||!owns(context)||nestedHistory.busy&&!applying)){reset();message='Предпросмотр отменён. Рассчитайте конвертацию заново.'}})
  $effect(()=>()=>{alive=false;reset()})
  async function calculate(){
    if(blocked||!project||!clip||!position||!node||!authState.token)return
    if(nestedEffectPreview.value&&nestedEffectPreview.value.owner!==owner){message='Завершите другое редактирование на кадре';return}
    reset();message='Расчёт…';pending=true
    const context:Context={base:project,signature:JSON.stringify(project),scope:compositionId,clipId,projectId:nestedComposition.projectId,generation:nestedAgentGeneration(),token:authState.token,time:nestedComposition.timeUs,parameters,controller:new AbortController(),prefix:`conversion-${crypto.randomUUID()}`};work=context
    try{
      const rate=node.content.canvas.fpsRate??{numerator:node.content.canvas.fpsMilli,denominator:1000}
      const last=Math.round((nativeClipEnd(clip)-nativeClipStart(clip))/1e6*rate.numerator/rate.denominator),end=endFrame??last
      if(!Number.isSafeInteger(startFrame)||!Number.isSafeInteger(end)||startFrame<0||end<=startFrame||end>last||!Number.isFinite(tolerance)||tolerance<0.05||!Number.isSafeInteger(interval)||interval<1)throw new Error('Проверьте диапазон кадров, допуск и интервал')
      const request=packPositionConversion(position,{fpsNum:rate.numerator,fpsDen:rate.denominator,startFrame,endFrame:end,tolerancePx:tolerance,everyFrames:position.mode==='path'&&mode==='bake'?interval:1,xyMode:position.mode==='path'?mode:'editable'})
      const answer=await previewPositionConversion(request,context.controller.signal)
      if(!owns(context))return
      if(!Number.isFinite(answer.maxErrorPx)||answer.maxErrorPx<0)throw new Error('Некорректная ошибка конвертации')
      message=`Ошибка ${answer.maxErrorPx.toFixed(3)} px · ${answer.nodeCount} узлов · ${answer.checkCount} проверок${answer.limit?` · ${answer.limit}`:''}`
      result=answer
      if(answer.withinTolerance){
        const candidate=nativePositionConversionCandidate(context.base,context.scope,context.clipId,answer,context.prefix)
        nestedEffectPreview.value={owner,gesture:context,base:context.base,project:candidate,timeUs:context.time,compositionId:context.scope,token:context.token}
      }
    }catch(error){if(owns(context)){result=null;clearEffectPreview(owner);message=error instanceof Error?error.message:String(error)}}
    finally{if(work===context)pending=false}
  }
  async function apply(){
    const context=work,answer=result
    if(!context||!answer?.withinTolerance||!owns(context)||blocked||pending)return
    applying=true
    try{
      const committed=await commitNestedPositionConversion(context.scope,context.clipId,answer,context.prefix,()=>{if(!owns(context))throw new Error('Контекст конвертации изменился')},context.controller.signal)
      if(committed){opened=false;reset()}
      else if(owns(context)){message=nestedHistory.error||'Конвертация не применена';result=null;clearEffectPreview(owner)}
    }catch(error){if(owns(context)){message=error instanceof Error?error.message:String(error);result=null;clearEffectPreview(owner)}}
    finally{applying=false}
  }
  function toggle(){opened=!opened;if(!opened)reset();message=''}
</script>
{#if transform}
  <button type="button" class="btn ghost sm" disabled={!opened&&blocked} onclick={toggle}>{opened?'Отменить конвертацию позиции':position?.mode==='path'?'Конвертировать движение в XY':'Конвертировать движение в Path'}</button>
  {#if opened}
    <fieldset disabled={blocked}>
      <legend>Конвертация движения</legend>
      <label>Начальный кадр<input aria-label="Нативная конвертация: начальный кадр" type="number" min="0" step="1" bind:value={startFrame} /></label>
      <label>Конечный кадр<input aria-label="Нативная конвертация: конечный кадр" type="number" min="1" step="1" placeholder="Конец клипа" bind:value={endFrame} /></label>
      <label>Допуск, px<input aria-label="Нативная конвертация: допуск" type="number" min="0.05" step="0.05" bind:value={tolerance} /></label>
      {#if position?.mode==='path'}
        <label>Режим<select aria-label="Нативная конвертация: режим" value={mode} onchange={event=>mode=event.currentTarget.value as 'editable'|'bake'}><option value="editable">Редактируемые ключи</option><option value="bake">Ключи по кадрам</option></select></label>
        <label>Интервал кадров<input aria-label="Нативная конвертация: интервал" type="number" min="1" step="1" bind:value={interval} /></label>
      {/if}
      <button type="button" class="btn ghost sm" disabled={pending} onclick={calculate}>Рассчитать конвертацию позиции</button>
      <button type="button" class="btn ghost sm" disabled={pending||!result?.withinTolerance||!work} onclick={apply}>Применить конвертацию позиции</button>
    </fieldset>
    <p aria-live="polite">{message}</p>
  {/if}
{/if}
<style>fieldset{border:0;padding:.5rem 0}label{display:flex;gap:.4rem;align-items:center;margin:.3rem 0}input{width:7rem}</style>
