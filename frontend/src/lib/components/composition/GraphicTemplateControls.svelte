<script lang="ts">
  import {getGraphicTemplateValues,editGraphicTemplate,type NativeGraphicTemplateControl} from '$lib/api'
  import {graphicShapeChoices,graphicTextChoices} from '$lib/composition/graphicLayerProperties'
  import type {GraphicClip,GraphicLayer} from '$lib/composition/types'
  let {clip,onchange,relativeUs=0,durationUs=1,animationOffsetUs=0}:{clip:GraphicClip;onchange:(layers:readonly GraphicLayer[])=>void;relativeUs?:number;durationUs?:number;animationOffsetUs?:number}=$props()
  let busy=$state(false)
  let error=$state('')
  let sampledValues=$state<Record<string,unknown>>({})
  let reading=$state(false),readReady=$state(false)
  const needsNativeValues=$derived(clip.graphic.layers.some(layer=>Boolean(layer.originalParameters || layer.content.animation || (layer.content.kind==='text' && layer.content.sourceTextAnimation))))
  $effect(()=>{
    const template=clip.template,graphic=clip.graphic,roll=clip.roll??null,responsiveTime=clip.responsiveTime??null
    const clock={relativeUs,durationUs,animationOffsetUs}
    if(!template || !needsNativeValues){sampledValues={};reading=false;readReady=true;return}
    const controller=new AbortController();reading=true;readReady=false;error=''
    void getGraphicTemplateValues({...template,graphic,roll,responsiveTime},clock,controller.signal).then(values=>{if(!controller.signal.aborted){sampledValues=values;readReady=true}}).catch(reason=>{if(!controller.signal.aborted)error=String(reason)}).finally(()=>{if(!controller.signal.aborted)reading=false})
    return ()=>controller.abort()
  })
  function value(control:NativeGraphicTemplateControl):unknown {
    if(needsNativeValues && readReady && Object.hasOwn(sampledValues,control.id))return sampledValues[control.id]
    const layer=clip.graphic.layers.find(layer=>layer.uid===control.layer)
    if(control.param==='enabled')return layer?.enabled ?? false
    const key=control.param.replace(/_([a-z])/g,(_,ch:string)=>ch.toUpperCase())
    return (layer?.content.parameters as Record<string,unknown>|undefined)?.[key]
  }
  function colorHex(control:NativeGraphicTemplateControl):string {
    const current=value(control)
    if(!Array.isArray(current))return typeof current==='string'?current:''
    const byte=(component:unknown)=>Math.round(Math.min(1,Math.max(0,Number(component)||0))*255).toString(16).padStart(2,'0')
    return '#'+current.slice(0,3).map(byte).join('')+(Number(current[3]??1)<.999?byte(current[3]):'')
  }
  function options(control:NativeGraphicTemplateControl):readonly string[]|undefined {
    const layer=clip.graphic.layers.find(layer=>layer.uid===control.layer)
    const key=control.param.replace(/_([a-z])/g,(_,ch:string)=>ch.toUpperCase())
    return (layer?.content.kind==='text'?graphicTextChoices:graphicShapeChoices).find(([field])=>field===key)?.[3]
  }
  function fontPart(control:NativeGraphicTemplateControl,part:'family'|'style'):string {
    const current=value(control)
    if(current && typeof current==='object' && !Array.isArray(current))return String((current as Record<string,unknown>)[part]??'')
    if(part==='family')return String(current??'')
    const layer=clip.graphic.layers.find(layer=>layer.uid===control.layer)
    return String((layer?.content.parameters as Record<string,unknown>|undefined)?.fontStyle??'')
  }
  async function edit(control:NativeGraphicTemplateControl,value:unknown){
    if(!clip.template || busy)return
    const snapshot=JSON.stringify({clip,relativeUs,durationUs,animationOffsetUs})
    busy=true;error=''
    try {
      const result=await editGraphicTemplate({...clip.template,graphic:clip.graphic,roll:clip.roll??null,responsiveTime:clip.responsiveTime??null},control.id,value,undefined,{relativeUs,durationUs,animationOffsetUs})
      if(snapshot!==JSON.stringify({clip,relativeUs,durationUs,animationOffsetUs}))throw new Error('Шаблон изменился; повторите ввод')
      onchange(result.graphic.layers)
    }catch(reason){error=String(reason)}finally{busy=false}
  }
  function component(control:NativeGraphicTemplateControl,index:number,raw:string){
    const number=Number(raw)
    if(!raw.trim()||!Number.isFinite(number)){error='Укажите конечное число';return}
    const old=value(control), values=Array.isArray(old)?[...old]:Array.from({length:control.kind==='color'?4:2},()=>0)
    values[index]=number;void edit(control,values)
  }
</script>
{#if clip.template}
<details open>
  <summary>{clip.template.name}</summary>
  <fieldset disabled={busy || reading || (needsNativeValues && !readReady)}>
    {#each clip.template.controls as control (control.id)}
      {#if control.kind==='checkbox'}
        <label><input type="checkbox" aria-label={`Шаблон ${control.id}`} checked={Boolean(value(control))} onchange={event=>void edit(control,event.currentTarget.checked)} />{control.name}</label>
      {:else if control.kind==='text'}
        <label>{control.name}<textarea aria-label={`Шаблон ${control.id}`} value={String(value(control)??'')} onchange={event=>void edit(control,event.currentTarget.value)}></textarea></label>
      {:else if control.kind==='font'}
        <label>{control.name}<input aria-label={`Шаблон ${control.id}`} value={fontPart(control,'family')} onchange={event=>void edit(control,{family:event.currentTarget.value})} /></label>
        <label>Начертание<input aria-label={`Шаблон ${control.id} начертание`} value={fontPart(control,'style')} onchange={event=>void edit(control,{style:event.currentTarget.value})} /></label>
      {:else if control.kind==='slider' && options(control)}
        <label>{control.name}<select aria-label={`Шаблон ${control.id}`} value={Number(value(control)??0)} onchange={event=>void edit(control,Number(event.currentTarget.value))}>{#each options(control)??[] as option,index}<option value={index} disabled={(control.min!=null&&index<control.min)||(control.max!=null&&index>control.max)}>{option}</option>{/each}</select></label>
      {:else if control.kind==='slider'}
        <label>{control.name}<input type="number" step="any" min={control.min??undefined} max={control.max??undefined} aria-label={`Шаблон ${control.id}`} value={Number(value(control)??0)} onchange={event=>{if(event.currentTarget.value.trim())void edit(control,Number(event.currentTarget.value))}} /></label>
      {:else}
        {#if control.kind==='color'}<label>HEX<input aria-label={`Шаблон ${control.id} HEX`} value={colorHex(control)} onchange={event=>void edit(control,event.currentTarget.value)} /></label>{/if}
        <div class="vector"><span>{control.name}</span>{#each (control.kind==='color'?['R','G','B','A']:['X','Y']) as axis,index}
          <label>{axis}<input type="number" step="any" aria-label={`Шаблон ${control.id} ${axis}`} value={Number((value(control) as number[]|undefined)?.[index]??0)} onchange={event=>component(control,index,event.currentTarget.value)} /></label>
        {/each}</div>
      {/if}
    {/each}
  </fieldset>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
{/if}
<style>
 fieldset {border:0;padding:0;margin:0;min-width:0} details {min-width:0} label {display:flex;flex-wrap:wrap;align-items:center;gap:.4rem;margin-top:.2rem;min-width:0} textarea {box-sizing:border-box;width:100%;min-width:0;max-width:100%;min-height:2.5rem} input:not([type=checkbox]) {min-width:0;width:7rem;max-width:100%} .vector {display:flex;gap:.3rem;flex-wrap:wrap;margin-top:.4rem} .vector>span {width:100%} .vector input {width:4.5rem} summary {font-weight:600}
</style>
