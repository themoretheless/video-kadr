<script lang="ts">
 import {exportNestedFramePng} from '$lib/api'
 import {nestedComposition,nestedHistory,nestedPlayback} from '$lib/state/nestedComposition.svelte'
 let {token,disabled=false}:{token:string;disabled?:boolean}=$props()
 let busy=$state(false),error=$state('')
 let controller:AbortController|undefined
 $effect(()=>{
  const project=nestedComposition.document,scope=nestedComposition.activeCompositionId,session=token
  void project;void scope;void session
  return()=>controller?.abort()
 })
 async function save(){
  const project=nestedComposition.document,scope=nestedComposition.activeCompositionId??project?.root,time=nestedComposition.timeUs
  if(!project||!scope||busy||disabled||nestedHistory.busy)return
  nestedPlayback.playing=false;nestedPlayback.epoch++
  const request=new AbortController();controller=request;busy=true;error=''
  try{
   const saved=nestedComposition.projectId&&nestedComposition.revision!==null?{projectId:nestedComposition.projectId,expectedRevision:nestedComposition.revision}:undefined
   const blob=await exportNestedFramePng(project,time,nestedComposition.renderedOutputs,token,request.signal,scope,saved)
   if(request.signal.aborted)return
   const url=URL.createObjectURL(blob),link=document.createElement('a')
   try{link.href=url;link.download=`frame-${scope}-${time}us.png`.replace(/[^\p{L}\p{N}._-]+/gu,'-');link.click()}finally{URL.revokeObjectURL(url)}
  }catch(cause){if(!request.signal.aborted)error=cause instanceof Error?cause.message:'Не удалось сохранить кадр'}
  finally{if(controller===request){controller=undefined;busy=false}}
 }
</script>
<button class="btn ghost sm" disabled={disabled||busy||nestedHistory.busy||!token||!nestedComposition.document} onclick={()=>void save()}>Сохранить кадр PNG</button>
{#if busy}<span role="status">Сохраняю кадр…</span>{/if}
{#if error}<p role="alert">{error}</p>{/if}
