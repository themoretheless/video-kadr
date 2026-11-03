<script lang="ts">
 import {untrack} from 'svelte'
 import type {CompositionSource} from '$lib/composition/types'
 import {importConcatProject,importConcatProjectWithSavedStills,importConcatProjectWithSavedCutouts,getCompositionProjects,fillConcatMediaSlot,type ConcatProjectImportReport,type ConcatFontBinding} from '$lib/api'
 import {concatImportMedia,concatImportFonts,concatImportStillDestinations,concatImportVideoDestinations,savedStillChoices,savedVideoChoices,type SavedStillChoice,type ConcatImportFont,type ConcatImportMedia,type ConcatImportSource} from '$lib/composition/concatProjectImport'
 import {adoptImportedConcatProject,nestedComposition,nestedHistory} from '$lib/state/nestedComposition.svelte'
 let {token=null,sources=[],context,disabled=false}:{token?:string|null;sources?:readonly ConcatImportSource[];context?:unknown;disabled?:boolean}=$props()
 let document=$state<unknown>(null),media=$state<readonly ConcatImportMedia[]>([]),bindings=$state<Record<string,string>>({}),report=$state<ConcatProjectImportReport|null>(null),busy=$state(false),error=$state(''),applied=$state(false)
 let fonts=$state<readonly ConcatImportFont[]>([]),fontBindings=$state<Record<string,ConcatFontBinding>>({}),fontNames=$state<Record<string,string>>({})
 let savedMasks=$state<readonly SavedStillChoice[]>([]),maskChoices=$state<Record<string,string>>({})
 let savedVideoMasks=$state<readonly SavedStillChoice[]>([])
 const videoDestinations=$derived(concatImportVideoDestinations(document))
 const stillDestinations=$derived(concatImportStillDestinations(document))
 function chosenMask(key:string,mediaId:string,video=false){return (video?savedVideoMasks:savedMasks).find(mask=>mask.key===maskChoices[key]&&mask.sourceId===bindings[mediaId])}
 let preserveMediaIds=$state(false)
 let slotChoices=$state<Record<string,string>>({})
 let sourceOverrides=$state<Record<string,CompositionSource>>({})
 let slotUndo=$state<{document:unknown;bindings:Record<string,string>;sourceOverrides:Record<string,CompositionSource>}[]>([])
 let fontEpoch=0,fontTicket=0
 let fontTickets:Record<string,number>=Object.create(null)
 let request=0,controller:AbortController|undefined,expectedContext='',lastToken=untrack(()=>token)
 const ready=$derived(!!document&&media.every(item=>!!bindings[item.id])&&fonts.every(font=>Object.hasOwn(fontBindings,font.path))&&stillDestinations.every(item=>!!chosenMask(item.key,item.mediaId))&&videoDestinations.every(item=>!!chosenMask(item.key,item.mediaId,true)))
 function identity(){return JSON.stringify([token,context,nestedComposition.document,nestedComposition.projectId,document,bindings,sourceOverrides,fontBindings,sources,preserveMediaIds,nestedComposition.revision,savedMasks,savedVideoMasks,maskChoices])}
 function invalidate(){request++;controller?.abort();report=null;applied=false;error='';busy=false}
 $effect(()=>{const current=token;if(current!==lastToken)untrack(()=>{lastToken=current;savedMasks=[];savedVideoMasks=[];maskChoices={};invalidate()})})
 $effect(()=>()=>{request++;fontEpoch++;controller?.abort()})
 async function load(file:File|undefined){invalidate();fontEpoch++;fontTickets=Object.create(null);fonts=[];fontBindings={};fontNames={};document=null;media=[];bindings={};slotChoices={};sourceOverrides={};slotUndo=[];maskChoices={};preserveMediaIds=false;if(!file)return;const serial=request;try{if(file.size>4*1024*1024)throw new Error('Файл Concat превышает 4 МиБ');const parsed:unknown=JSON.parse(await file.text());if(serial!==request)return;media=concatImportMedia(parsed);fonts=concatImportFonts(parsed);document=parsed}catch(failure){if(serial===request)error=failure instanceof Error?failure.message:String(failure)}}
 function fallbackFont(path:string){invalidate();fontTickets[path]=++fontTicket;fontNames={...fontNames,[path]:'Системная замена'};fontBindings={...fontBindings,[path]:{fallback:true}}}
 async function loadFont(path:string,file:File|undefined){
  invalidate();const epoch=fontEpoch,ticket=++fontTicket,currentDocument=document,currentToken=token;fontTickets[path]=ticket
  const remaining={...fontBindings};delete remaining[path];fontBindings=remaining
  const names={...fontNames};delete names[path];fontNames=names
  if(!file)return
  const current=()=>epoch===fontEpoch&&fontTickets[path]===ticket&&document===currentDocument&&token===currentToken
  try{
   if(file.size>8*1024*1024)throw new Error('Шрифты проекта превышают 8 МиБ')
   const bytes=new Uint8Array(await file.arrayBuffer());if(!current())return
   const decoded=(data:string)=>data.length/4*3-(data.endsWith('==')?2:data.endsWith('=')?1:0)
   if(bytes.length+Object.values(fontBindings).reduce((n,binding)=>n+('data' in binding?decoded(binding.data):0),0)>8*1024*1024)throw new Error('Шрифты проекта превышают 8 МиБ')
   let binary='';for(let at=0;at<bytes.length;at+=32768)binary+=String.fromCharCode(...bytes.subarray(at,at+32768))
   fontNames={...fontNames,[path]:file.name};fontBindings={...fontBindings,[path]:{data:btoa(binary)}}
  }catch(failure){if(current())error=failure instanceof Error?failure.message:String(failure)}
 }
 async function fillSlot(mediaId:string){
  if(!document||busy||disabled||nestedHistory.busy||!token)return
  const entry=sources.find(entry=>entry.source.id===slotChoices[mediaId])
  if(!entry||!media.some(item=>item.id===mediaId&&item.placeholder))return
  invalidate();busy=true;const serial=request,before=document,beforeBindings={...bindings},beforeOverrides={...sourceOverrides},scope=identity();controller=new AbortController()
  try{
   const result=await fillConcatMediaSlot(before,mediaId,entry,controller.signal,token)
   if(serial!==request)return
   if(identity()!==scope)throw new Error('Текущий проект изменился; повторите заполнение слота')
   const nextMedia=concatImportMedia(result.document)
   const filled=nextMedia.find(item=>item.id===mediaId)
   if(!filled||filled.placeholder||filled.kind!==result.source.kind)throw new Error('Ответ не содержит заполненный слот')
   slotUndo=[...slotUndo.slice(-19),{document:before,bindings:beforeBindings,sourceOverrides:beforeOverrides}]
   document=result.document;media=nextMedia;bindings={...bindings,[mediaId]:entry.source.id};sourceOverrides={...sourceOverrides,[mediaId]:result.source}
  }catch(failure){if(serial===request)error=failure instanceof Error?failure.message:String(failure)}finally{if(serial===request)busy=false}
 }
 function undoSlot(){if(busy||disabled||nestedHistory.busy)return;const previous=slotUndo.at(-1);if(!previous)return;invalidate();slotUndo=slotUndo.slice(0,-1);document=previous.document;bindings=previous.bindings;sourceOverrides=previous.sourceOverrides;media=concatImportMedia(document)}
 async function loadSavedMasks(){
  if(!token||busy||disabled||nestedHistory.busy)return
  invalidate();savedMasks=[];savedVideoMasks=[];maskChoices={};busy=true;const serial=request,scope=identity();controller=new AbortController()
  try{const projects=await getCompositionProjects(token,controller.signal);if(serial!==request)return;if(identity()!==scope)throw new Error('Контекст импорта изменился; обновите список масок');savedMasks=savedStillChoices(projects);savedVideoMasks=savedVideoChoices(projects)}
  catch(failure){if(serial===request)error=failure instanceof Error?failure.message:String(failure)}finally{if(serial===request)busy=false}
 }
 async function convert(){if(!ready||busy||disabled||nestedHistory.busy||!token)return;invalidate();busy=true;const serial=request;controller=new AbortController();expectedContext=identity();try{const mapped=Object.fromEntries(media.map(item=>{const entry=sources.find(entry=>entry.source.id===bindings[item.id]);if(!entry)throw new Error('Выбранный источник исчез из библиотеки');return[item.id,sourceOverrides[item.id]?.id===entry.source.id?sourceOverrides[item.id]:entry.source]}));const references=stillDestinations.map(item=>{const selected=chosenMask(item.key,item.mediaId);if(!selected)throw new Error('Выберите сохранённую маску для каждого изображения');return{...selected.reference,timelineId:item.timelineId,clipId:item.clipId}});const videoReferences=videoDestinations.map(item=>{const selected=chosenMask(item.key,item.mediaId,true);if(!selected)throw new Error('Выберите сохранённую маску для каждого видео');return{...selected.reference,timelineId:item.timelineId,clipId:item.clipId}});const result=videoReferences.length?await importConcatProjectWithSavedCutouts(document,mapped,references,videoReferences,controller.signal,token,fontBindings,preserveMediaIds):references.length?await importConcatProjectWithSavedStills(document,mapped,references,controller.signal,token,fontBindings,preserveMediaIds):preserveMediaIds?await importConcatProject(document,mapped,controller.signal,token,fontBindings,true):fonts.length?await importConcatProject(document,mapped,controller.signal,token,fontBindings):await importConcatProject(document,mapped,controller.signal,token);if(serial!==request)return;if(identity()!==expectedContext)throw new Error('Текущий проект изменился; повторите проверку импорта');report=result}catch(failure){if(serial===request)error=failure instanceof Error?failure.message:String(failure)}finally{if(serial===request)busy=false}}
 async function apply(){if(!report?.project||busy||disabled||nestedHistory.busy||applied)return;const selected=report,serial=request;if(identity()!==expectedContext){error='Текущий проект изменился; повторите проверку импорта';return}const name=typeof document==='object'&&document&&'name'in document&&typeof document.name==='string'?document.name:'Импорт Concat';busy=true;const accepted=await adoptImportedConcatProject(selected.project!,name,()=>{if(serial!==request||identity()!==expectedContext)throw new Error('Контекст импорта изменился')});busy=false;if(accepted)applied=true;else error=nestedHistory.error||'Импорт не применён'}
 function download(){if(!report)return;const url=URL.createObjectURL(new Blob([JSON.stringify(report,null,2)],{type:'application/json'}));const link=globalThis.document.createElement('a');link.href=url;link.download='concat-import-report.json';link.click();setTimeout(()=>URL.revokeObjectURL(url),0)}
</script>
<details><summary>Импорт проекта Concat</summary>
 <fieldset disabled={disabled||busy||nestedHistory.busy||!token}>
  <label>Файл Concat JSON <input type="file" accept=".json,application/json" aria-label="Файл проекта Concat" onchange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';void load(file)}} /></label>
  {#each media as item(item.id)}
   <label>{item.name} <select aria-label={`Источник Concat ${item.id}`} value={bindings[item.id]??''} onchange={event=>{invalidate();bindings={...bindings,[item.id]:event.currentTarget.value}}}><option value="">Выберите медиа из библиотеки</option>{#each sources.filter(entry=>(entry.source.kind===item.kind||sourceOverrides[item.id]?.id===entry.source.id&&sourceOverrides[item.id]?.kind===item.kind)) as entry(entry.source.id)}<option value={entry.source.id}>{entry.label}</option>{/each}</select></label>
   {#if item.placeholder}
    <label>Заполнить слот {item.name} <select aria-label={`Заполнение слота Concat ${item.id}`} value={slotChoices[item.id]??''} onchange={event=>{slotChoices={...slotChoices,[item.id]:event.currentTarget.value}}}><option value="">Выберите замену из библиотеки</option>{#each sources as entry(entry.source.id)}<option value={entry.source.id}>{entry.label}</option>{/each}</select></label>
    <button class="btn ghost sm" type="button" disabled={!slotChoices[item.id]} aria-label={`Заполнить слот Concat ${item.id}`} onclick={()=>void fillSlot(item.id)}>Заполнить слот</button>
   {/if}
  {/each}
  {#if slotUndo.length}<button class="btn ghost sm" type="button" onclick={undoSlot}>Отменить заполнение слота</button><p>Слот заполнен в исходном документе. Проверьте импорт: неподдерживаемые параметры будут показаны в отчёте.</p>{/if}
  {#each fonts as font(font.path)}
   <label>Шрифт {font.family} <input type="file" accept=".ttf,.otf,.ttc" aria-label={`Файл шрифта Concat ${font.path}`} onchange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';void loadFont(font.path,file)}} />{#if Object.hasOwn(fontBindings,font.path)}<span>{fontNames[font.path]}</span>{/if}</label><button class="btn ghost sm" type="button" aria-label={`Системная замена шрифта Concat ${font.path}`} onclick={()=>fallbackFont(font.path)}>Использовать системную замену</button>
  {/each}
  {#if fonts.length}<p>Выберите файлы шрифтов. Они сохранятся внутри проекта.</p>{/if}
  {#if media.length}<p>Недостающие файлы сначала добавьте в библиотеку.</p>{/if}
  {#if media.length}<label><input type="checkbox" aria-label="Сохранять отдельные ID медиа Concat" checked={preserveMediaIds} onchange={event=>{invalidate();preserveMediaIds=event.currentTarget.checked}} />Сохранять отдельные слоты при общем файле</label>{/if}
  {#if stillDestinations.length||videoDestinations.length}
   <p>Для каждого изображения и видео с вырезанием выберите сохранённую маску с теми же настройками.</p>
   <button class="btn ghost sm" type="button" onclick={()=>void loadSavedMasks()}>Обновить сохранённые маски</button>
   {#each stillDestinations as item(item.key)}
    <label>Маска {item.label} <select aria-label={`Сохранённая маска Concat ${item.timelineId} ${item.clipId}`} value={maskChoices[item.key]??''} onchange={event=>{invalidate();maskChoices={...maskChoices,[item.key]:event.currentTarget.value}}}><option value="">Выберите сохранённую маску</option>{#each savedMasks.filter(mask=>mask.sourceId===bindings[item.mediaId]) as mask(mask.key)}<option value={mask.key}>{mask.label}</option>{/each}</select></label>
   {/each}
   {#each videoDestinations as item(item.key)}
    <label>Видеомаска {item.label} <select aria-label={`Сохранённая видеомаска Concat ${item.timelineId} ${item.clipId}`} value={maskChoices[item.key]??''} onchange={event=>{invalidate();maskChoices={...maskChoices,[item.key]:event.currentTarget.value}}}><option value="">Выберите сохранённую видеомаску</option>{#each savedVideoMasks.filter(mask=>mask.sourceId===bindings[item.mediaId]) as mask(mask.key)}<option value={mask.key}>{mask.label}</option>{/each}</select></label>
   {/each}
   {#if !savedMasks.length&&!savedVideoMasks.length}<p>Сначала выполните вырезание в проекте и сохраните его, затем обновите список масок.</p>{/if}
  {/if}
  <button class="btn ghost sm" disabled={!ready} onclick={()=>void convert()}>Проверить импорт Concat</button>
  {#if report?.status==='converted'&&report.project}<p>Таймлайнов: {Object.keys(report.project.compositions).length}. Открытие создаст новый несохранённый проект; прежний доступен через undo.</p><button class="btn primary sm" disabled={applied} onclick={()=>void apply()}>Открыть импорт как новый проект</button>{/if}
  {#if report}<button class="btn ghost sm" onclick={download}>Скачать исходник и отчёт импорта</button>{/if}
 </fieldset>
 {#if report?.diagnostics.length}<ul>{#each report.diagnostics as message,index (index)}<li>{message}</li>{/each}</ul>{/if}
 {#if error}<p role="alert">{error}</p>{/if}{#if applied}<p>Импорт открыт. Для сохранения используйте «Сохранить».</p>{/if}
</details>
<style>fieldset{border:0;padding:0.5rem 0;display:flex;gap:0.5rem;flex-wrap:wrap;align-items:center}label{display:flex;gap:0.5rem;align-items:center}p{flex-basis:100%}select{max-width:24rem}</style>
