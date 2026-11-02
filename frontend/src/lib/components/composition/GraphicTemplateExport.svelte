<script lang="ts">
 import {exportGraphicTemplateFile,type NativeGraphicTemplate,type NativeGraphicTemplateControl} from '$lib/api'
 import {graphicShapeChannels,graphicTextChannels,graphicShapeFlags,graphicTextFlags,graphicShapeChoices,graphicTextChoices,setGraphicNumeric} from '$lib/composition/graphicLayerProperties'
 import type {GraphicClip} from '$lib/composition/types'
 let {clip}:{clip:GraphicClip}=$props()
 let embedFonts=$state(false),fontLicense=$state('UNKNOWN'),autoTextControls=$state(true)
 let controls=$state<NativeGraphicTemplateControl[]>([]), target=$state('')
 const labels:Record<string,string>={text:'Текст',font:'Шрифт',fontStyle:'Начертание',position:'Положение',anchor:'Опорная точка',size:'Размер',opacity:'Непрозрачность',fillColor:'Цвет заливки',strokeColor:'Цвет обводки',rotation:'Поворот',scale:'Масштаб',enabled:'Видимость'}
 const choices=$derived(clip.graphic.layers.flatMap((layer,index)=>{
  const text=layer.content.kind==='text'
  let defaults=layer.content
  for(const [channel,,,,fallback] of text?graphicTextChannels:graphicShapeChannels)defaults=setGraphicNumeric(defaults,channel,fallback,clip.width,clip.height)
  const flags=Object.fromEntries((text?graphicTextFlags:graphicShapeFlags).map(([field,,value])=>[field,value]))
  const discrete=Object.fromEntries((text?graphicTextChoices:graphicShapeChoices).map(([field,,value])=>[field,value]))
  const params:Record<string,unknown>={...discrete,enabled:true,name:'',...(text?{text:'',font:'',fontStyle:''}:{}),...defaults.parameters,...flags,...layer.content.parameters}
  // Auto-position is an editable vector even when the saved value is null.
  if(params.position==null)params.position=[clip.width/2,clip.height/2]
  return Object.entries(params).flatMap(([field,value])=>{
   const kind:NativeGraphicTemplateControl['kind']|null=field==='font'?'font':typeof value==='string'?'text':typeof value==='boolean'?'checkbox':typeof value==='number'?'slider':Array.isArray(value)&&value.length===2&&value.every(v=>typeof v==='number')?'position':Array.isArray(value)&&value.length===4&&value.every(v=>typeof v==='number')?'color':null
   if(!kind||!layer.uid)return []
   const param=field.replace(/[A-Z]/g,c=>'_'+c.toLowerCase())
   return [{key:`${layer.uid}:${param}`,layer:layer.uid,param,kind,label:`Слой ${index+1} · ${labels[field]??field}`} ]
  })
 }))
 function addControl(){const choice=choices.find(c=>c.key===target);if(!choice)return;let n=1;while(controls.some(c=>c.id===`control_${n}`))n++;controls=[...controls,{id:`control_${n}`,name:choice.label,kind:choice.kind,layer:choice.layer,param:choice.param}]}
 const controlKinds:readonly NativeGraphicTemplateControl['kind'][]=['text','font','slider','checkbox','color','position']
 function renameControl(index:number,input:HTMLInputElement){
  const id=input.value.trim()
  if(!id||controls.some((control,i)=>i!==index&&control.id===id)){error='Идентификатор контрола должен быть непустым и уникальным';input.value=controls[index]!.id;return}
  error='';controls=controls.map((control,i)=>i===index?{...control,id}:control)
 }
 function moveControl(index:number,delta:number){
  const destination=index+delta
  if(destination<0||destination>=controls.length)return
  const next=[...controls];[next[index],next[destination]]=[next[destination]!,next[index]!];controls=next
 }
 let name=$state(''),category=$state('My Templates'),currentId=$state(''),busy=$state(false),error=$state('')
 $effect(()=>{const id=clip.id,initialName=clip.template?.name??'Мой шаблон',initialCategory=clip.template?.category??'My Templates';if(currentId!==id){currentId=id;name=initialName;category=initialCategory;controls=(clip.template?.controls??[]).map(c=>({...c}));target=''}})
 async function save(){
  if(busy)return
  if(!name.trim()){error='Укажите название шаблона';return}
  const template:NativeGraphicTemplate={id:clip.template?.id??`user:${clip.id}`,name:name.trim(),category,description:clip.template?.description??'',author:clip.template?.author??'',license:clip.template?.license??'',tags:clip.template?.tags??[],canvas:[clip.width,clip.height],durationUs:clip.durationTicks,graphic:clip.graphic,roll:clip.roll??null,responsiveTime:clip.responsiveTime??null,controls,...(clip.template?.resources?{resources:clip.template.resources}:{})}
  busy=true;error=''
  try{const options={...(embedFonts?{embedFonts:true,fontLicense}:{}),...(!autoTextControls?{autoTextControls:false}:{})};const text=Object.keys(options).length?await exportGraphicTemplateFile(template,clip.animationOffsetUs??0,undefined,options):await exportGraphicTemplateFile(template,clip.animationOffsetUs??0);const document=JSON.parse(text) as {id:string};const file=(document.id.replace(/^user:/,'').replace(/[^a-zA-Z0-9_-]/g,'-')||'graphic')+'.fcgt';const url=URL.createObjectURL(new Blob([text],{type:'application/json'}));try{const link=window.document.createElement('a');link.href=url;link.download=file;link.click()}finally{setTimeout(()=>URL.revokeObjectURL(url),1000)}}catch(reason){error=String(reason)}finally{busy=false}
 }
</script>
<details>
 <summary>Сохранить графику как шаблон</summary>
 <fieldset disabled={busy}>
  <label>Название<input aria-label="Название экспортируемого шаблона" bind:value={name} /></label>
  <label>Категория<input aria-label="Категория экспортируемого шаблона" bind:value={category} /></label>
  <label>Новый редактируемый параметр<select aria-label="Параметр нового контрола" value={target} onchange={event=>{target=event.currentTarget.value}}><option value="">Выберите параметр</option>{#each choices as choice}<option value={choice.key}>{choice.label}</option>{/each}</select></label>
  <button type="button" disabled={!target} onclick={addControl}>Добавить контрол</button>
  {#each controls as control,index (control.id)}
   <div class="control">
    <label>Идентификатор<input aria-label={`Идентификатор контрола ${index+1}`} value={control.id} onchange={event=>renameControl(index,event.currentTarget)} /></label>
    <label>Тип<select aria-label={`Тип контрола ${index+1}`} value={control.kind} onchange={event=>{const kind=event.currentTarget.value as NativeGraphicTemplateControl['kind'];if(controlKinds.includes(kind))controls=controls.map((c,i)=>i===index?{...c,kind}:c)}}>{#each controlKinds as kind}<option value={kind}>{kind}</option>{/each}</select></label>

    <label>Название контрола<input aria-label={`Название контрола ${index+1}`} value={control.name} oninput={event=>{controls=controls.map((c,i)=>i===index?{...c,name:event.currentTarget.value}:c)}} /></label>
    {#if control.kind==='slider'}
     <label>Минимум<input type="number" aria-label={`Минимум контрола ${index+1}`} value={control.min??''} oninput={event=>{const raw=event.currentTarget.value;controls=controls.map((c,i)=>i===index?{...c,min:raw===''?null:Number(raw)}:c)}} /></label>
     <label>Максимум<input type="number" aria-label={`Максимум контрола ${index+1}`} value={control.max??''} oninput={event=>{const raw=event.currentTarget.value;controls=controls.map((c,i)=>i===index?{...c,max:raw===''?null:Number(raw)}:c)}} /></label>
    {/if}
    <button class="btn ghost sm" type="button" aria-label={`Контрол ${index+1} выше`} disabled={index===0} onclick={()=>moveControl(index,-1)}>Выше</button>
    <button class="btn ghost sm" type="button" aria-label={`Контрол ${index+1} ниже`} disabled={index===controls.length-1} onclick={()=>moveControl(index,1)}>Ниже</button>
    <button type="button" aria-label={`Удалить контрол ${index+1}`} onclick={()=>{controls=controls.filter((_,i)=>i!==index)}}>Удалить</button>
   </div>
  {/each}
  {#if controls.length===0}<label><span><input type="checkbox" aria-label="Автоматически добавить текстовые контролы" bind:checked={autoTextControls} />Автоматически добавить текстовые контролы</span></label>{/if}
  <label><span><input type="checkbox" aria-label="Встроить шрифты в шаблон" bind:checked={embedFonts} />Встроить шрифты</span></label>
  {#if embedFonts}<label>Лицензия шрифтов<input aria-label="Лицензия встраиваемых шрифтов" bind:value={fontLicense} /></label>{/if}
  <button class="btn ghost sm" type="button" onclick={()=>void save()}>Скачать графику .fcgt</button>
 </fieldset>
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>summary{cursor:pointer;font-size:.8rem}fieldset{border:0;margin:0;padding:0;min-width:0}label{display:grid;gap:.25rem;margin-top:.3rem}.control{margin-top:.4rem;padding-top:.3rem;border-top:1px solid var(--border)}select{width:100%;min-width:0}input[type=checkbox]{width:auto}input{min-width:0;width:100%;box-sizing:border-box}p{overflow-wrap:anywhere}</style>
