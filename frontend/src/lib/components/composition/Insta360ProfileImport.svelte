<script lang="ts">
 import {inspectInsta360Profile,type Insta360ProfileAnalysis} from '$lib/api'
 let {sourceId,token,binding,disabled=false,onapply}:{sourceId:string;token:string;binding:unknown;disabled?:boolean;onapply:(analysis:Insta360ProfileAnalysis)=>void}=$props()
 let busy=$state(false),error=$state(''),result=$state<Insta360ProfileAnalysis|null>(null)
 let controller:AbortController|undefined
 $effect(()=>()=>controller?.abort())
 async function run(){
  if(disabled||busy)return
  const original=binding,owner=JSON.stringify([sourceId,token]),active=new AbortController();controller=active;busy=true;error='';result=null
  try{
   const analysis=await inspectInsta360Profile(sourceId,token,active.signal)
   if(active.signal.aborted)return
   if(disabled)throw new Error("Редактирование клипа заблокировано")
   if(binding!==original||owner!==JSON.stringify([sourceId,token]))throw new Error('Клип изменился; повторите чтение профиля')
   result=analysis
   if(analysis?.calibration.renderReady&&analysis.calibration.profile&&analysis.sourceLayout)onapply(analysis)
  }catch(cause){if(!active.signal.aborted)error=cause instanceof Error?cause.message:String(cause)}finally{if(controller===active){controller=undefined;busy=false}}
 }
</script>
<details><summary>Автоматический профиль Insta360</summary>
 <p>Калибровка берётся из исходного файла. Для двух объективов проверяются параметры обоих видеопотоков.</p>
 <button class="btn ghost sm" disabled={disabled||busy} onclick={()=>void run()}>Применить профиль из камеры</button>
 {#if busy}<button class="btn ghost sm" onclick={()=>controller?.abort()}>Отмена</button>{/if}
 {#if result}
  {#if result.calibration.renderReady&&result.calibration.profile&&result.sourceLayout}<p role="status">Применена индивидуальная калибровка двух объективов. Сшивка использует оба видеопотока.</p>
  {:else}<p role="status">Автоматическое применение недоступно: {result.calibration.reason} {result.packingReason}</p>{/if}
 {/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
