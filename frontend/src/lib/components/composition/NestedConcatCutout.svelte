<script lang="ts">
 import {nestedComposition,nestedHistory,analyseNestedImageCutout} from '$lib/state/nestedComposition.svelte'
 import type {ConcatCutoutSubject} from '$lib/api'
 let {token,compositionId,clipId,savedCutout,disabled=false}:{token:string|null;compositionId:string;clipId:string;savedCutout?:import('$lib/composition/types').CutoutAttachment;disabled?:boolean}=$props()
 let subject=$state<ConcatCutoutSubject>('auto'),feather=$state(0.01),busy=$state(false)
 let controller:AbortController|undefined
 const scope=$derived(JSON.stringify([token,nestedComposition.projectId,compositionId,clipId]))
 $effect(()=>{void scope;subject=savedCutout?.subject??'auto';feather=savedCutout?.correction.feather??0.01;return()=>{controller?.abort()}})
 async function run(){if(!token||busy||disabled)return;const request=new AbortController();controller=request;busy=true;try{await analyseNestedImageCutout(token,compositionId,clipId,subject,feather,request.signal)}finally{if(controller===request)busy=false}}
</script>
<fieldset disabled={disabled||busy||nestedHistory.busy||!token||!nestedComposition.projectId||nestedComposition.revision===null}>
 <legend>Удаление фона · Concat</legend>
 <label>Объект<select aria-label="Concat cutout subject" value={subject} onchange={event=>{subject=event.currentTarget.value as ConcatCutoutSubject}}><option value="auto">Автоматически</option><option value="person">Человек</option><option value="object">Предмет</option></select></label>
 <label>Растушёвка<input aria-label="Concat cutout feather" type="number" min="0" max="0.1" step="0.001" bind:value={feather}/></label>
 <button class="btn ghost sm" type="button" onclick={()=>void run()}>{busy?'Удаляется фон…':savedCutout?'Повторить анализ фона':'Удалить фон с выбором модели'}</button>
</fieldset>
{#if busy}<button class="btn ghost sm" type="button" onclick={()=>controller?.abort()}>Отменить анализ</button>{/if}
