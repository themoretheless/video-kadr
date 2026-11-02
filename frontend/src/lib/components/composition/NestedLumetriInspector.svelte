<script lang="ts">
 import {stepComparisonReference} from '$lib/composition/comparison'
 import {matchNestedShot} from '$lib/composition/colorMatchProbe'
 import ToneCurveChannel from './ToneCurveChannel.svelte'
 import {exportColorLut,listBuiltinLuts,validateColorLut,type NativeBuiltinLut} from '$lib/api'
 import {nestedComparison,nestedComposition,nestedHistory,removeNestedProjectLut,updateNestedProjectLut,importNestedProjectLut,editNestedVideoEffects,applyNestedColorMatch} from '$lib/state/nestedComposition.svelte'
 import type {WireLumetri,WireAnimatableValue,WireColorLut,WireToneCurves} from '$lib/composition/types'
 let {disabled=false,token=''}:{disabled?:boolean;token?:string}=$props()
 let skinProtect=$state(true),matching=$state(false),matchStatus=$state('')
 function stepReference(direction:-1|1):void {
  const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas
  if(!canvas)return
  try{nestedComparison.referenceUs=stepComparisonReference(nestedComparison.referenceUs,direction,canvas)}catch(failure){error=failure instanceof Error?failure.message:String(failure)}
 }
 async function match(track:string,clip:string,index:number):Promise<void> {
  if(disabled||matching||importing||nestedHistory.busy||!token||!nestedComposition.document)return
  const document=nestedComposition.document,composition=compositionId,time=nestedComposition.timeUs
  const identity=JSON.stringify(document)
  matching=true;error='';matchStatus='';request?.abort();const abort=new AbortController();request=abort
  try {
   const result=await matchNestedShot(document,composition,track,clip,index,time,nestedComparison.referenceUs,nestedComposition.renderedOutputs,token,skinProtect,abort.signal)
   if(!active||abort.signal.aborted)return
   if(nestedComposition.document!==document||JSON.stringify(nestedComposition.document)!==identity||compositionId!==composition)throw new Error('Документ изменился: повторите Color Match')
   await applyNestedColorMatch(composition,track,clip,index,result)
   if(nestedHistory.error)throw new Error(nestedHistory.error)
   matchStatus=`Ошибка цвета: ${result.before.toFixed(4)} → ${result.after.toFixed(4)}`
  }catch(failure){if(active&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally{if(active&&request===abort)matching=false}
 }
 let importing=$state(false),exporting=$state(false),error=$state('')
 let builtins=$state<readonly NativeBuiltinLut[]>([])
 let builtinsLoaded=$state(false)
 $effect(()=>{
  if(builtinsLoaded)return
  if(!tracks.some(t=>(t.kind==='video'||t.kind==='image')&&t.clips.some(c=>(c.effects ?? []).some(e=>e.kind==='lumetri'))))return
  const abort=new AbortController();let current=true
  void listBuiltinLuts(abort.signal).then(entries=>{if(current){builtins=entries;builtinsLoaded=true}}).catch(failure=>{if(current&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)})
  return ()=>{current=false;abort.abort()}
 })
 let request:AbortController|undefined
 let active=true
 $effect(()=>{active=true;return ()=>{active=false;request?.abort()}})
 async function replaceLibrary(id:string,file:File|undefined):Promise<void> {
  if(!file||disabled||matching||importing||exporting||nestedHistory.busy)return
  const document=nestedComposition.document
  importing=true;error='';request?.abort();const abort=new AbortController();request=abort
  try {
   if(file.size>16*1024*1024)throw new Error('LUT превышает 16 МБ')
   const ext=file.name.split('.').at(-1)?.toLowerCase()
   if(ext!=='cube'&&ext!=='3dl')throw new Error('Выберите .cube или .3dl')
   const lut:WireColorLut={format:ext==='cube'?'cube':'three_dl',text:await file.text(),intensity:100}
   if(active&&!abort.signal.aborted&&nestedComposition.document===document)await updateNestedProjectLut(id,{lut,sourcePath:file.name},abort.signal)
   if(nestedHistory.error)throw new Error(nestedHistory.error)
  }catch(failure){if(active&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally{if(active&&request===abort)importing=false}
 }
 async function exportTable(parameters:WireLumetri,role:'inputLut'|'lookLut',format:'cube'|'three_dl'):Promise<void> {
  if(disabled||matching||importing||exporting||nestedHistory.busy)return
  const builtin=parameters[role==='inputLut'?'inputLutBuiltin':'lookLutBuiltin']
  const library=parameters[role==='inputLut'?'inputLutLibrary':'lookLutLibrary']
  const inline=parameters[role]
  const source=builtin?{reference:`builtin:${builtin}`} : library?{reference:`lib:${library}`,luts:nestedComposition.document?.luts ?? []} : inline?{lut:inline}:null
  if(!source)return
  exporting=true;error='';request?.abort();const abort=new AbortController();request=abort
  try {
   const result=await exportColorLut(source,format,abort.signal)
   if(!active||abort.signal.aborted)return
   const url=URL.createObjectURL(new Blob([result.text],{type:'text/plain;charset=utf-8'}))
   const anchor=document.createElement('a');anchor.href=url;anchor.download=result.filename;anchor.hidden=true;document.body.append(anchor)
   try{anchor.click()}finally{anchor.remove();setTimeout(()=>URL.revokeObjectURL(url),0)}
  }catch(failure){if(active&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally{if(active&&request===abort)exporting=false}
 }
 async function load(file:File|undefined,track:string,clip:string,index:number,role:'inputLut'|'lookLut',shared=false):Promise<void> {
  if(!file||importing||disabled||nestedHistory.busy)return
  const composition=compositionId,document=nestedComposition.document
  importing=true;error='';request?.abort();const abort=new AbortController();request=abort
  try {
   if(file.size>16*1024*1024)throw new Error('LUT превышает 16 МБ')
   const ext=file.name.split('.').at(-1)?.toLowerCase()
   if(ext!=='cube'&&ext!=='3dl')throw new Error('Выберите .cube или .3dl')
   const lut:WireColorLut={format:ext==='cube'?'cube':'three_dl',text:await file.text(),intensity:100}
   await validateColorLut(lut,abort.signal)
   if(active&&!abort.signal.aborted&&nestedComposition.document===document&&compositionId===composition&&shared)await importNestedProjectLut(lut,file.name.replace(/\.(cube|3dl)$/i,''),file.name,composition,track,clip,index,role==='inputLut',abort.signal)
   if(active&&!abort.signal.aborted&&nestedComposition.document===document&&compositionId===composition&&!shared)await editNestedVideoEffects(composition,track,clip,effects=>effects.map((e,i)=>i===index&&e.kind==='lumetri'?{...e,parameters:{...e.parameters,[role]:lut,[role==='inputLut'?'inputLutBuiltin':'lookLutBuiltin']:null,[role==='inputLut'?'inputLutLibrary':'lookLutLibrary']:null}}:e))
   if(nestedHistory.error)throw new Error(nestedHistory.error)
  }catch(failure){if(active&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally{if(active&&request===abort)importing=false}
 }
 let curveChannel=$state<keyof WireToneCurves>('master')
 const curveLabels:Record<keyof WireToneCurves,string>={master:'Общая',red:'Красный',green:'Зелёный',blue:'Синий',hueVsSat:'Оттенок → насыщенность',hueVsHue:'Оттенок → оттенок',hueVsLuma:'Оттенок → светлота',lumaVsSat:'Светлота → насыщенность',satVsSat:'Насыщенность → насыщенность'}
 const emptyPoints:readonly (readonly [number,number])[]=[]
 type Section='basic'|'creative'|'vignette'|'secondary'|'sharpen'|'grade'
 const sections:readonly {id:Section;label:string;channels:readonly (readonly [string,string,number,number,number])[]}[]=[
 {id:'basic',label:'Базовая коррекция',channels:[['temperature','Температура',-100,100,0],['tint','Оттенок',-100,100,0],['exposure','Экспозиция',-5,5,0],['contrast','Контраст',-100,100,0],['highlights','Света',-100,100,0],['shadows','Тени',-100,100,0],['whites','Белые',-100,100,0],['blacks','Чёрные',-100,100,0],['saturation','Насыщенность',0,200,100]]},
 {id:'creative',label:'Творческие настройки',channels:[['fadedFilm','Выцветание',0,100,0],['vibrance','Красочность',-100,100,0],['saturation','Насыщенность',0,200,100],['shadowRed','Тени R',0,1,0.5],['shadowGreen','Тени G',0,1,0.5],['shadowBlue','Тени B',0,1,0.5],['highlightRed','Света R',0,1,0.5],['highlightGreen','Света G',0,1,0.5],['highlightBlue','Света B',0,1,0.5]]},
 {id:'vignette',label:'Виньетка',channels:[['amount','Сила',-5,5,0],['midpoint','Середина',0,100,50],['roundness','Округлость',-100,100,0],['feather','Растушёвка',0,100,50]]},
 {id:'secondary',label:'HSL Secondary',channels:[['hue','Тон',0,360,0],['hueRange','Диапазон',1,180,30],['saturationMin','Минимум насыщенности',0,100,10],['lightnessMin','Минимум яркости',0,100,5],['lightnessMax','Максимум яркости',0,100,95],['soften','Мягкость',0,100,20],['denoise','Шумоподавление',0,100,0],['blur','Размытие',0,100,0],['temperature','Температура',-100,100,0],['tint','Оттенок',-100,100,0],['saturation','Насыщенность',0,200,100],['hueShift','Сдвиг тона',-180,180,0]]},
 {id:'grade',label:'Стиль и цветовые колёса',channels:[['intensity','Интенсивность',0,200,100],['shadowsX','Тени X',-1,1,0],['shadowsY','Тени Y',-1,1,0],['shadowsLightness','Тени яркость',-100,100,0],['midtonesX','Средние X',-1,1,0],['midtonesY','Средние Y',-1,1,0],['midtonesLightness','Средние яркость',-100,100,0],['highlightsX','Света X',-1,1,0],['highlightsY','Света Y',-1,1,0],['highlightsLightness','Света яркость',-100,100,0]]},
 {id:'sharpen',label:'Резкость',channels:[['amount','Сила',-100,100,0]]}]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(p:WireLumetri)=>WireLumetri):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((e,i)=>i===index && e.kind==='lumetri'?{...e,parameters:change(e.parameters)}:e))
 }
 function wheelKey(key:string):readonly ['shadows'|'midtones'|'highlights','x'|'y'|'lightness']|undefined {
  for(const wheel of ['shadows','midtones','highlights'] as const)for(const [suffix,axis] of [['X','x'],['Y','y'],['Lightness','lightness']] as const)if(key===wheel+suffix)return [wheel,axis]
 }
 function value(p:WireLumetri,s:Section,key:string,base:number):number {
  if(s==='grade'){const pair=wheelKey(key);if(pair)return p.grade?.[pair[0]]?.[pair[1]] ?? base;return p.grade?.intensity ?? base}
  const v=(p[s] as Readonly<Record<string,unknown>>|undefined)?.[key];return typeof v==='number'?v:base
 }
 function setValue(p:WireLumetri,s:Section,key:string,v:number):WireLumetri {
  if(s==='grade'){const pair=wheelKey(key);return {...p,grade:pair?{...p.grade,[pair[0]]:{...p.grade?.[pair[0]],[pair[1]]:v}}:{...p.grade,intensity:v}}}
  return {...p,[s]:{...p[s],[key]:v}}
 }

 function animation(p:WireLumetri,s:Section,key:string):WireAnimatableValue|undefined {
  if(s==='sharpen')return p.animation?.sharpen ?? undefined
  return (p.animation?.[s] as Readonly<Record<string,WireAnimatableValue>>|undefined)?.[key]
 }
 function keyframe(p:WireLumetri,s:Section,key:string,base:number,start:number):WireLumetri {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=animation(p,s,key)
  if(previous?.mode==='keyframes'&&previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(q=>q.tick!==tick),{...points.find(q=>q.tick===tick),tick,value:value(p,s,key,base)}].sort((a,b)=>a.tick-b.tick)
  const track:WireAnimatableValue={mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}
  return {...p,animation:{...p.animation,[s]:s==='sharpen'?track:{...p.animation?.[s],[key]:track}}}
 }
 function removeAnimation(p:WireLumetri,s:Section,key:string):WireLumetri {
  if(s==='sharpen')return {...p,animation:{...p.animation,sharpen:null}}
  const channels={...p.animation?.[s]} as Record<string,WireAnimatableValue>
  delete channels[key]
  return {...p,animation:{...p.animation,[s]:channels}}
 }
</script>
<section aria-label="Lumetri">
 {#if nestedComposition.document?.luts?.length}
 <details><summary>Библиотека LUT проекта</summary>
 <fieldset disabled={disabled || matching || importing || exporting || nestedHistory.busy}>
 {#each nestedComposition.document.luts as table (table.id)}
 <label>Имя<input aria-label={`LUT ${table.id} имя`} value={table.name} maxlength="1024" onchange={e=>void updateNestedProjectLut(table.id,{name:e.currentTarget.value})} /></label>
 <button class="btn ghost sm" aria-label={`LUT ${table.id} удалить из библиотеки`} onclick={()=>void removeNestedProjectLut(table.id)}>Удалить из библиотеки</button>
 <label>Заменить таблицу<input type="file" accept=".cube,.3dl" aria-label={`LUT ${table.id} заменить`} onchange={e=>{const file=e.currentTarget.files?.[0];e.currentTarget.value='';void replaceLibrary(table.id,file)}} /></label>
 {/each}
 </fieldset></details>
 {/if}

 {#each tracks as track (track.id)}{#if track.kind==='video' || track.kind==='image'}{#each track.clips as clip (clip.id)}
 <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || matching || importing || exporting || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,e=>[...e,{kind:'lumetri',parameters:{}}])}>Добавить Lumetri</button></div>
 {#each (clip.effects ?? []) as effect,index}{#if effect.kind==='lumetri'}
 <fieldset disabled={disabled || matching || importing || exporting || nestedHistory.busy || track.locked}><legend>Lumetri {index+1}</legend>
 <label>Включён <input type="checkbox" checked={effect.parameters.enabled ?? true} onchange={e=>{const enabled=e.currentTarget.checked;edit(track.id,clip.id,index,p=>({...p,enabled}))}} /></label>
 <details><summary>Color Match</summary>
 <label>Эталон, мкс <input aria-label={`${clip.id} Lumetri эталон`} type="number" min="0" step="1" bind:value={nestedComparison.referenceUs} /></label>
 <label>Сравнение кадров <input aria-label="Lumetri сравнение кадров" type="checkbox" bind:checked={nestedComparison.enabled} /></label>
 <div class="row"><button class="btn ghost sm" onclick={()=>stepReference(-1)}>Предыдущий эталонный кадр</button><button class="btn ghost sm" onclick={()=>stepReference(1)}>Следующий эталонный кадр</button><button class="btn ghost sm" onclick={()=>{nestedComparison.referenceUs=nestedComposition.timeUs}}>Текущий кадр как эталон</button></div>
 <label>Защита кожи <input type="checkbox" bind:checked={skinProtect} /></label>
 <button class="btn ghost sm" disabled={!token} onclick={()=>void match(track.id,clip.id,index)}>Применить Color Match</button>
 </details>
 <details><summary>LUT</summary>
 {#each ['inputLut','lookLut'] as role}
 {@const key=role as 'inputLut'|'lookLut'}
 {@const builtinKey=key==='inputLut'?'inputLutBuiltin':'lookLutBuiltin'}
 {@const libraryKey=key==='inputLut'?'inputLutLibrary':'lookLutLibrary'}
 <label>{key==='inputLut'?'Камерная таблица':'Творческая таблица'}<select aria-label={`${clip.id} Lumetri ${key} встроенный`} value={effect.parameters[builtinKey] ?? ''} onchange={e=>{const id=e.currentTarget.value;edit(track.id,clip.id,index,p=>({...p,[builtinKey]:id||null,...(id?{[key]:null,[libraryKey]:null}:{})}))}}><option value="">Нет</option>{#each builtins.filter(b=>key==='lookLut'||b.input) as table}<option value={table.id}>{table.label}</option>{/each}</select></label>

 {#if nestedComposition.document?.luts?.length || effect.parameters[libraryKey]}
 <label>Библиотека проекта<select aria-label={`${clip.id} Lumetri ${key} библиотека`} value={effect.parameters[libraryKey] ?? ''} onchange={e=>{const id=e.currentTarget.value;edit(track.id,clip.id,index,p=>({...p,[libraryKey]:id||null,...(id?{[key]:null,[builtinKey]:null}:{})}))}}><option value="">Нет</option>{#if effect.parameters[libraryKey] && !(nestedComposition.document?.luts ?? []).some(l=>l.id===effect.parameters[libraryKey])}<option value={effect.parameters[libraryKey] ?? ''}>Таблица удалена: {effect.parameters[libraryKey]}</option>{/if}{#each nestedComposition.document?.luts ?? [] as table}<option value={table.id}>{table.name}</option>{/each}</select></label>
 {/if}
 <label>Импорт в библиотеку<input type="file" accept=".cube,.3dl" aria-label={`${clip.id} Lumetri ${key} импорт в библиотеку`} onchange={e=>{const file=e.currentTarget.files?.[0];e.currentTarget.value='';void load(file,track.id,clip.id,index,key,true)}} /></label>
 <label>{key==='inputLut'?'Входной LUT':'Творческий LUT'}<input type="file" accept=".cube,.3dl" aria-label={`${clip.id} Lumetri ${key} импорт`} onchange={e=>{const file=e.currentTarget.files?.[0];e.currentTarget.value='';void load(file,track.id,clip.id,index,key)}} /></label>
 {#if effect.parameters[key] || effect.parameters[builtinKey] || effect.parameters[libraryKey]}
 <div class="row"><button class="btn ghost sm" aria-label={`${clip.id} Lumetri ${key} экспорт cube`} onclick={()=>void exportTable(effect.parameters,key,'cube')}>Скачать .cube</button><button class="btn ghost sm" aria-label={`${clip.id} Lumetri ${key} экспорт 3dl`} onclick={()=>void exportTable(effect.parameters,key,'three_dl')}>Скачать .3dl</button></div>
 {/if}
 {#if effect.parameters[key]}<button class="btn ghost sm" aria-label={`${clip.id} Lumetri ${key} удалить`} onclick={()=>edit(track.id,clip.id,index,p=>({...p,[key]:null}))}>Удалить LUT</button>{/if}
 {/each}</details>
 <details><summary>Кривые</summary>
 <label>Включены <input aria-label={`${clip.id} Lumetri кривые включены`} type="checkbox" checked={effect.parameters.curvesOn ?? true} onchange={e=>{const curvesOn=e.currentTarget.checked;edit(track.id,clip.id,index,p=>({...p,curvesOn}))}} /></label>
 <label>Канал <select aria-label={`${clip.id} Lumetri канал кривой`} value={curveChannel} onchange={e=>{const next=e.currentTarget.value;if(Object.hasOwn(curveLabels,next))curveChannel=next as keyof WireToneCurves}}>{#each Object.entries(curveLabels) as [key,label]}<option value={key}>{label}</option>{/each}</select></label>
 <ToneCurveChannel points={effect.parameters.curves?.[curveChannel] ?? emptyPoints} label={curveLabels[curveChannel]} periodic={!['master','red','green','blue'].includes(curveChannel)} disabled={disabled || matching || importing || exporting || nestedHistory.busy || track.locked} onchange={points=>{const channel=curveChannel;edit(track.id,clip.id,index,p=>({...p,curves:{...p.curves,[channel]:points}}))}} />
 </details>
 {#each sections as section}
 <details><summary>{section.label}</summary>
 {#if section.id!=='sharpen' && section.id!=='grade'}<label>Включено <input aria-label={`${clip.id} Lumetri ${section.id} включено`} type="checkbox" checked={effect.parameters[section.id]?.enabled ?? section.id!=='secondary'} onchange={e=>{const enabled=e.currentTarget.checked;edit(track.id,clip.id,index,p=>({...p,[section.id]:{...p[section.id],enabled}}))}} /></label>{/if}
 {#if section.id==='secondary'}
 <label>Просмотр <select aria-label={`${clip.id} Lumetri HSL просмотр`} value={effect.parameters.secondary?.showMask ?? 0} onchange={e=>{const showMask=Number(e.currentTarget.value);edit(track.id,clip.id,index,p=>({...p,secondary:{...p.secondary,showMask}}))}}><option value="0">Коррекция</option><option value="1">Цвет / серый</option><option value="2">Цвет / чёрный</option><option value="3">Белый / чёрный</option></select></label>
 {/if}
 {#if section.id==='grade'}
 <label>Стиль <select aria-label={`${clip.id} Lumetri стиль`} value={effect.parameters.grade?.look ?? 0} onchange={e=>{const look=Number(e.currentTarget.value);edit(track.id,clip.id,index,p=>({...p,grade:{...p.grade,look}}))}}>{#each ['Нет','Teal & Orange','Warm Film','Cool Blue','Bleach Bypass','Faded Matte','Monochrome','Golden Hour','Night'] as name,i}<option value={i}>{name}</option>{/each}</select></label>
 <label>Колёса включены <input type="checkbox" checked={effect.parameters.wheelsOn ?? true} onchange={e=>{const wheelsOn=e.currentTarget.checked;edit(track.id,clip.id,index,p=>({...p,wheelsOn}))}} /></label>
 {/if}
 <div class="channels">{#each section.channels as [key,label,min,max,base]}<label>{label}<input aria-label={`${clip.id} Lumetri ${section.id} ${key}`} type="number" {min} {max} step="0.1" value={value(effect.parameters,section.id,key,base)} onchange={e=>{const v=Number(e.currentTarget.value);edit(track.id,clip.id,index,p=>setValue(p,section.id,key,v))}} /></label><button class="btn ghost sm" aria-label={`${clip.id} Lumetri ${section.id} ${key} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,p=>keyframe(p,section.id,key,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
 {#if animation(effect.parameters,section.id,key)}<button class="btn ghost sm" aria-label={`${clip.id} Lumetri ${section.id} ${key} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,p=>removeAnimation(p,section.id,key))}>Удалить анимацию</button>{/if}
 {/each}</div>
 </details>{/each}
 <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({}))}>Сбросить Lumetri</button>
 <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,e=>e.filter((_,i)=>i!==index))}>Удалить Lumetri</button>
 </fieldset>{/if}{/each}{/each}{/if}{/each}
 {#if matching}<p role="status">Подбираю цвет…</p>{/if}
 {#if matchStatus}<p role="status">{matchStatus}</p>{/if}
 {#if importing}<p role="status">Проверяю LUT…</p>{/if}
 {#if error || nestedHistory.error}<p role="alert">{error || nestedHistory.error}</p>{/if}
</section>
<style>.row{display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap}fieldset{border:0;padding:0.5rem 0;margin:0}.channels{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:0.25rem}input[type=number]{width:4.5rem}label{display:flex;justify-content:space-between;gap:0.5rem}</style>
