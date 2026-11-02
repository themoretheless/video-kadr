<script lang="ts">
  import { nestedComposition,nestedHistory,nestedAgentGeneration,editNestedScalarKeys,pasteNestedScalarKeys } from '$lib/state/nestedComposition.svelte'
  import { nestedScalarClipboard } from '$lib/state/nestedScalarClipboard.svelte'
  import { nestedEffectPreview,clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
  import { captureNativeScalarClipboard,planNativeScalarPaste,nativeScalarPasteClips,serializeNativeScalarClipboard,parseNativeScalarClipboard } from '$lib/composition/nativeScalarClipboard'
  import { nativeScalarSelectionOperations,type NativeScalarKeySelection } from '$lib/composition/nativeScalarTimeline'
  import { authState } from '$lib/state/auth.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  import type { NativeNestedProject } from '$lib/api'
  let {selection,onselection,disabled=false,onactivate=()=>{}}:{selection:readonly NativeScalarKeySelection[];onselection:(keys:NativeScalarKeySelection[])=>void;disabled?:boolean;onactivate?:()=>void}=$props()
  const project=$derived(nestedComposition.document),scope=$derived(nestedComposition.activeCompositionId??project?.root??'')
  const clips=$derived(project?nativeScalarPasteClips(project,scope):[])
  let targetId=$state(''),pending=$state(false),message=$state('')
  const target=$derived(clips.find(clip=>clip.id===targetId)??clips[0])
  const owner=Symbol('native scalar clipboard')
  type Context={base:NativeNestedProject;signature:string;scope:string;projectId:string|null;generation:number;token:string;time:number;selection:string;clipboard:typeof nestedScalarClipboard.value;clipboardText:string;target:string;controller:AbortController}
  let work=$state.raw<Context|null>(null),io=$state.raw<Context|null>(null),preview=$state.raw<{context:Context;plan:ReturnType<typeof planNativeScalarPaste>}|null>(null),alive=true
  const unavailable=$derived(disabled||!!agentSession.lock||!authState.token)
  const blocked=$derived(unavailable||pending||nestedHistory.busy||!!io)
  function owns(context:Context){return alive&&work===context&&project===context.base&&JSON.stringify(project)===context.signature&&scope===context.scope&&nestedComposition.projectId===context.projectId&&nestedAgentGeneration()===context.generation&&authState.token===context.token&&nestedComposition.timeUs===context.time&&JSON.stringify(selection)===context.selection&&target?.id===context.target&&nestedScalarClipboard.value===context.clipboard&&JSON.stringify(nestedScalarClipboard.value)===context.clipboardText&&!unavailable&&!context.controller.signal.aborted&&(!nestedEffectPreview.value||nestedEffectPreview.value.owner===owner)}
  function cancel(){work?.controller.abort();work=null;io=null;preview=null;clearEffectPreview(owner)}
  function begin():Context|undefined{
    if(blocked||!project||!target||!authState.token)return
    onactivate();cancel();message=''
    if(nestedEffectPreview.value&&nestedEffectPreview.value.owner!==owner){message='Завершите другое редактирование на кадре';return}
    const context={base:project,signature:JSON.stringify(project),scope,projectId:nestedComposition.projectId,generation:nestedAgentGeneration(),token:authState.token,time:nestedComposition.timeUs,selection:JSON.stringify(selection),clipboard:nestedScalarClipboard.value,clipboardText:JSON.stringify(nestedScalarClipboard.value),target:target.id,controller:new AbortController()};work=context;return context
  }
  $effect(()=>{const context=work;if(context&&(!owns(context)||nestedHistory.busy&&!pending))cancel()})
  $effect(()=>()=>{alive=false;cancel()})
  function guard(context:Context){if(!owns(context))throw new Error('Контекст буфера ключей изменился')}
  export async function copy(cut=false){
    if(blocked||!project||!selection.length)return
    const context=begin();if(!context)return
    try{
      const packet=captureNativeScalarClipboard(context.base,context.scope,selection)
      if(!cut){nestedScalarClipboard.value=packet;message='Ключи скопированы';cancel();return}
      const operations=nativeScalarSelectionOperations(context.base,context.scope,selection,'delete')
      pending=true
      const accepted=await editNestedScalarKeys(context.scope,operations,()=>guard(context),context.controller.signal,()=>{nestedScalarClipboard.value=packet})
      if(accepted){if(alive&&scope===context.scope)onselection([]);message='Ключи вырезаны'}
      else if(work===context)message=nestedHistory.error||'Ключи не вырезаны'
    }catch(error){if(work===context)message=error instanceof Error?error.message:String(error)}
    finally{pending=false;if(work===context)cancel()}
  }
  function previewPaste(){
    const context=begin();if(!context)return
    try{
      if(!context.clipboard)throw new Error('Буфер ключей пуст')
      const plan=planNativeScalarPaste(context.base,context.scope,context.clipboard,context.target,context.time)
      preview={context,plan}
      nestedEffectPreview.value={owner,gesture:context,base:context.base,project:plan.project,timeUs:context.time,compositionId:context.scope,token:context.token}
      message=`Ключей для вставки: ${plan.selection.length}${plan.skipped.length?` · пропущено каналов: ${plan.skipped.length}`:''}`
    }catch(error){cancel();message=error instanceof Error?error.message:String(error)}
  }
  async function applyPaste(){
    const prepared=preview;if(!prepared||blocked||!owns(prepared.context)||!prepared.context.clipboard)return
    pending=true
    const context=prepared.context
    try{
      const accepted=await pasteNestedScalarKeys(context.scope,context.target,context.clipboard!,context.time,()=>guard(context),context.controller.signal)
      if(accepted){if(alive&&scope===context.scope)onselection(prepared.plan.selection);message='Ключи вставлены'}
      else if(work===context)message=nestedHistory.error||'Ключи не вставлены'
    }catch(error){if(work===context)message=error instanceof Error?error.message:String(error)}
    finally{pending=false;if(work===context)cancel()}
  }
  export async function paste(){if(blocked)return;previewPaste();await applyPaste()}
  async function system(read:boolean){
    const context=begin();if(!context)return
    io=context
    try{
      if(read){
        if(!navigator.clipboard?.readText)throw new Error('Браузер не предоставляет чтение системного буфера')
        const text=await navigator.clipboard.readText();guard(context)
        const packet=parseNativeScalarClipboard(text);guard(context);nestedScalarClipboard.value=packet;message='Системный буфер ключей прочитан'
      }else{
        if(!context.clipboard)throw new Error('Буфер ключей пуст')
        if(!navigator.clipboard?.writeText)throw new Error('Браузер не предоставляет запись системного буфера')
        await navigator.clipboard.writeText(serializeNativeScalarClipboard(context.clipboard));guard(context);message='Ключи записаны в системный буфер'
      }
    }catch(error){if(work===context)message=error instanceof Error?error.message:String(error)}
    finally{if(io===context)io=null;if(work===context)cancel()}
  }
</script>
<svelte:window onkeydown={event=>{if(event.key==='Escape'&&work)cancel()}} />
<fieldset disabled={blocked}>
  <legend>Буфер ключей</legend>
  <button type="button" class="btn ghost sm" disabled={!selection.length} onclick={()=>void copy()}>Копировать выбранные ключи</button>
  <button type="button" class="btn ghost sm" disabled={!selection.length} onclick={()=>void copy(true)}>Вырезать выбранные ключи</button>
  <label>Вставить в клип<select aria-label="Клип для вставки скалярных ключей" value={target?.id??''} onchange={event=>targetId=event.currentTarget.value}>{#each clips as clip (clip.id)}<option value={clip.id} disabled={clip.locked}>{clip.label}</option>{/each}</select></label>
  <button type="button" class="btn ghost sm" disabled={!nestedScalarClipboard.value||!target||target.locked} onclick={previewPaste}>Предпросмотр вставки ключей</button>
  <button type="button" class="btn ghost sm" disabled={!preview||!owns(preview.context)} onclick={()=>void applyPaste()}>Вставить скалярные ключи</button>
  <button type="button" class="btn ghost sm" onclick={()=>void system(true)}>Прочитать ключи из системного буфера</button>
  <button type="button" class="btn ghost sm" disabled={!nestedScalarClipboard.value} onclick={()=>void system(false)}>Записать ключи в системный буфер</button>
</fieldset>
{#if preview||io||pending}<button type="button" class="btn ghost sm" onclick={cancel}>Отменить операцию буфера ключей</button>{/if}
<p>Вставка начинается от курсора и заменяет ключи в совпавшее время. Ctrl/⌘+C, X, V на дорожках используют буфер ключей.</p>
{#if message}<p aria-live="polite">{message}</p>{/if}
<style>fieldset{border:0;padding:.5rem 0;display:flex;align-items:center;gap:.5rem;flex-wrap:wrap}label{display:flex;gap:.3rem}select{max-width:18rem}p{font-size:.8rem}</style>
