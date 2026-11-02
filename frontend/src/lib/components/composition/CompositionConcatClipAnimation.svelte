<script lang="ts">
 import type {WireConcatClipAnimation,WireConcatParamKey} from '$lib/composition/types'
 import {compositionState,editCompositionConcatClipKeys} from '$lib/state/composition.svelte'
 import Editor from './ConcatClipAnimationEditor.svelte'
 let {clipId,animation,disabled=false}:{clipId:string;animation:WireConcatClipAnimation;disabled?:boolean}=$props()
 let busy=$state(false),error=$state(''),generation=0
 $effect(()=>{const id=clipId;void id;generation++;busy=false;error=''})
 async function change(property:string,keys:readonly WireConcatParamKey[]){
  const id=clipId,request=++generation;busy=true;error=''
  try{await editCompositionConcatClipKeys(id,property,keys,()=>{if(request!==generation||clipId!==id||compositionState.ui.selectedClipId!==id||disabled)throw new Error('Выбранный клип изменился')})}
  catch(reason){if(request===generation)error=reason instanceof Error?reason.message:String(reason)}
  finally{if(request===generation)busy=false}
 }
</script>
<Editor {animation} disabled={disabled||busy} onchange={(property,keys)=>void change(property,keys)}/>
{#if error}<p role="alert">{error}</p>{/if}
