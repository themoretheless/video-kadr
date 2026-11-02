<script lang="ts">
 import {TRANSITION_DEFINITIONS,defaultProceduralTransition,type ProceduralTransitionSpec,type TransitionParameterValue} from '$lib/composition/transitionCatalog'
 let {value,onchange}:{value:ProceduralTransitionSpec;onchange:(value:ProceduralTransitionSpec)=>void}=$props()
 const definition=$derived(TRANSITION_DEFINITIONS.find(d=>d.id===value.effect))
 const controls=$derived((definition?.params??[]).map(parameter=>({...parameter,value:value.params?.[parameter.id]??parameter.default})))
 function set(id:string,parameter:TransitionParameterValue){onchange({...value,params:{...value.params,[id]:parameter}})}
 function range(kind:unknown){
  const spec=typeof kind==='object'&&kind!==null&&'Float' in kind?kind.Float as {min:number;max:number;decimals:number;step?:number}:null
  return {min:spec?.min,max:spec?.max,step:spec?(spec.step??10**-spec.decimals):1}
 }
 function choices(kind:unknown):readonly string[]{return typeof kind==='object'&&kind!==null&&'Choice' in kind?kind.Choice as string[]:[]}
 function point(id:string,axis:'x'|'y',text:string){
  const current=controls.find(p=>p.id===id)?.value
  if(current&&'Vec2' in current)set(id,{Vec2:{...current.Vec2,[axis]:text===''?null:Number(text)}})
 }
 function color(id:string,index:number,text:string){
  const current=controls.find(p=>p.id===id)?.value
  if(current&&'Color' in current){const next=[...current.Color];next[index]=Number(text);set(id,{Color:next})}
 }
</script>
<label>Ядро перехода
 <select aria-label="Ядро перехода" value={value.effect} onchange={(event)=>onchange(defaultProceduralTransition(event.currentTarget.value))}>
  {#each TRANSITION_DEFINITIONS as d(d.id)}<option value={d.id}>{d.category.at(-1)} · {d.name}</option>{/each}
 </select>
</label>
<div class="composition-form-grid">
 {#each controls as parameter(parameter.id)}
  {#if 'Float' in parameter.value}
   <label>{parameter.label}<input aria-label={`Параметр перехода ${parameter.id}`} type="number" {...range(parameter.kind)} value={parameter.value.Float} oninput={(event)=>set(parameter.id,{Float:Number(event.currentTarget.value)})}/></label>
  {:else if 'Choice' in parameter.value}
   <label>{parameter.label}<select aria-label={`Параметр перехода ${parameter.id}`} value={parameter.value.Choice} onchange={(event)=>set(parameter.id,{Choice:Number(event.currentTarget.value)})}>{#each choices(parameter.kind) as option,index}<option value={index}>{option}</option>{/each}</select></label>
  {:else if 'Bool' in parameter.value}
   <label><input aria-label={`Параметр перехода ${parameter.id}`} type="checkbox" checked={parameter.value.Bool} onchange={(event)=>set(parameter.id,{Bool:event.currentTarget.checked})}/>{parameter.label}</label>
  {:else if 'Vec2' in parameter.value}
   {#each ['x','y'] as axis}<label>{parameter.label} {axis}<input aria-label={`Параметр перехода ${parameter.id} ${axis}`} type="number" placeholder="Auto" value={parameter.value.Vec2[axis as 'x'|'y']??''} oninput={(event)=>point(parameter.id,axis as 'x'|'y',event.currentTarget.value)}/></label>{/each}
  {:else if 'Color' in parameter.value}
   {#each ['R','G','B','A'] as channel,index}<label>{parameter.label} {channel}<input aria-label={`Параметр перехода ${parameter.id} ${channel}`} type="number" min="0" max="1" step="0.01" value={parameter.value.Color[index]} oninput={(event)=>color(parameter.id,index,event.currentTarget.value)}/></label>{/each}
  {/if}
 {/each}
</div>
