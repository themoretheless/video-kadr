<script lang="ts">
  import {matchesGraphicTemplate,graphicTemplateCategories} from '$lib/composition/graphicTemplateSearch'
  import GraphicTemplateThumbnail from './GraphicTemplateThumbnail.svelte'
  import GraphicTemplateLibrary from './GraphicTemplateLibrary.svelte'
  import {getGraphicTemplates,type NativeGraphicTemplate} from '$lib/api'
  let {onchoose}:{onchoose:(template:NativeGraphicTemplate)=>void}=$props()
  let templates=$state<readonly NativeGraphicTemplate[]>([])
  let selected=$state(''),query=$state(''),category=$state('')
  const filtered=$derived(templates.filter(template=>matchesGraphicTemplate(template,query,category)))
  const selectedTemplate=$derived(filtered.find(template=>template.id===selected))
  $effect(()=>{if(!filtered.some(template=>template.id===selected))selected=filtered[0]?.id??''})
  let busy=$state(false)
  let error=$state('')
  async function load(){busy=true;error='';try {templates=await getGraphicTemplates();selected=templates[0]?.id ?? ''} catch(reason){error=String(reason)}finally{busy=false}}
</script>
<details>
  <summary>Шаблоны графики</summary>
  <button class="btn ghost sm" type="button" disabled={busy} onclick={()=>void load()}>{templates.length?'Обновить каталог':'Загрузить шаблоны'}</button>
  {#if templates.length}
    <label>Поиск<input aria-label="Поиск встроенных шаблонов" bind:value={query} /></label>
    <label>Категория<select aria-label="Категория встроенных шаблонов" value={category} onchange={event=>{category=event.currentTarget.value}}><option value="">Все категории</option>{#each graphicTemplateCategories(templates) as value}<option value={value}>{value}</option>{/each}</select></label>
    {#if !filtered.length}<p role="status">Шаблоны не найдены</p>{/if}
    <label>Шаблон<select aria-label="Графический шаблон" value={selected} onchange={event=>selected=event.currentTarget.value}>{#each filtered as template (template.id)}<option value={template.id}>{template.category} · {template.name}</option>{/each}</select></label>
    {#if selectedTemplate}<GraphicTemplateThumbnail template={selectedTemplate} name={selectedTemplate.name} />{/if}
    <p>{selectedTemplate?.description}</p>
    <button class="btn ghost sm" type="button" disabled={busy || !selectedTemplate} onclick={()=>{if(selectedTemplate)onchoose(selectedTemplate)}}>Добавить шаблон</button>
  {/if}
  <GraphicTemplateLibrary {onchoose} />
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>summary {cursor:pointer;font-size:.8rem} details {min-width:0;max-width:100%} label {display:grid;gap:.25rem;min-width:0} select {width:100%;min-width:0;max-width:100%} p {overflow-wrap:anywhere;font-size:.8rem}</style>
