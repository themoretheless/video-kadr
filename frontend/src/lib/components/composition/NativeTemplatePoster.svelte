<script lang="ts">
 import {getNativeTemplatePoster} from '$lib/api'
 let {id,name,token}:{id:string;name:string;token:string}=$props()
 let url=$state(''),failed=$state(false)
 $effect(()=>{
  const key=id,owner=token,request=new AbortController();let owned:string|undefined
  url='';failed=false
  void getNativeTemplatePoster(key,owner,request.signal).then(blob=>{if(!request.signal.aborted){owned=URL.createObjectURL(blob);url=owned}}).catch(()=>{if(!request.signal.aborted)failed=true})
  return()=>{request.abort();if(owned)URL.revokeObjectURL(owned)}
 })
</script>
{#if failed}<span>Обложка недоступна</span>{:else if url}<img src={url} alt={`Обложка ${name}`} width="160" height="90" onerror={()=>{failed=true}}/>{/if}
<style>img { object-fit: contain; max-width: 100%; }</style>
