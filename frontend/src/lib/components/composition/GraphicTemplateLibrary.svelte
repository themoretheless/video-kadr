<script lang="ts">
 import GraphicTemplateThumbnail from './GraphicTemplateThumbnail.svelte'
 import {matchesGraphicTemplate,graphicTemplateCategories} from '$lib/composition/graphicTemplateSearch'
 import {listUserGraphicTemplates,installUserGraphicTemplate,removeUserGraphicTemplate,writeGraphicTemplateFile,instantiateGraphicTemplateFile,type NativeGraphicTemplate,type UserGraphicTemplateFile} from '$lib/api'
 let {onchoose}:{onchoose?:(template:NativeGraphicTemplate)=>void}=$props()
 let query=$state(''),category=$state(''),replace=$state(false)
 let entries=$state<UserGraphicTemplateFile[]>([]),busy=$state(false),error=$state('')
 const filtered=$derived(entries.filter(entry=>matchesGraphicTemplate(entry.document,query,category)))
 async function run(action:()=>Promise<void>){if(busy)return;busy=true;error='';try{await action()}catch(reason){error=String(reason)}finally{busy=false}}
 async function refresh(){entries=await listUserGraphicTemplates()}
 async function upload(input:HTMLInputElement){const file=input.files?.[0];if(!file)return;await run(async()=>{if(!file.name.toLowerCase().endsWith('.fcgt'))throw new Error('Выберите шаблон Filmcraft .fcgt');const contents=await file.text();if(replace)await installUserGraphicTemplate(contents,undefined,{replace:true});else await installUserGraphicTemplate(contents);await refresh()});input.value=''}
 async function download(entry:UserGraphicTemplateFile){await run(async()=>{const text=await writeGraphicTemplateFile(entry.document);const url=URL.createObjectURL(new Blob([text],{type:'application/json'}));try{const link=document.createElement('a');link.href=url;link.download=entry.file;link.click()}finally{setTimeout(()=>URL.revokeObjectURL(url),1000)}})}
</script>
<details>
 <summary>Мои шаблоны .fcgt</summary>
 <fieldset disabled={busy}>
  <label>Установить шаблон<input aria-label="Установить шаблон fcgt" type="file" accept=".fcgt" onchange={event=>void upload(event.currentTarget)} /></label>
  <label><input type="checkbox" aria-label="Заменить одноимённый шаблон" bind:checked={replace} />Заменить одноимённый шаблон</label>
  <button class="btn ghost sm" type="button" onclick={()=>void run(refresh)}>Обновить мои шаблоны</button>
  <label>Поиск<input aria-label="Поиск моих шаблонов" bind:value={query} /></label>
  <label>Категория<select aria-label="Категория моих шаблонов" value={category} onchange={event=>{category=event.currentTarget.value}}><option value="">Все категории</option>{#each graphicTemplateCategories(entries.map(entry=>entry.document)) as value}<option value={value}>{value}</option>{/each}</select></label>
  {#if entries.length && !filtered.length}<p role="status">Шаблоны не найдены</p>{/if}
  {#each filtered as entry (entry.file)}
   <div><span>{String(entry.document.name??entry.file)}</span>
    <GraphicTemplateThumbnail document={entry.document} name={String(entry.document.name??entry.file)} />
    <button class="btn ghost sm" type="button" disabled={!onchoose} aria-label={`Добавить ${entry.file}`} onclick={()=>void run(async()=>{const template=await instantiateGraphicTemplateFile(entry.document);onchoose?.(template)})}>Добавить</button>
    <button class="btn ghost sm" type="button" aria-label={`Скачать ${entry.file}`} onclick={()=>void download(entry)}>Скачать</button>
    <button class="btn ghost sm" type="button" aria-label={`Удалить ${entry.file}`} onclick={()=>void run(async()=>{await removeUserGraphicTemplate(entry.file);await refresh()})}>Удалить</button>
   </div>
  {/each}
 </fieldset>
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>summary{cursor:pointer;font-size:.8rem} fieldset{border:0;padding:0;margin:0;min-width:0} label{display:grid;gap:.25rem} select{width:100%;min-width:0} input{min-width:0;max-width:100%;font-size:.8rem} div{display:flex;flex-wrap:wrap;gap:.25rem;margin-top:.4rem} span{width:100%;overflow-wrap:anywhere} p{overflow-wrap:anywhere}</style>
