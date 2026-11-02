<script lang="ts">
 import type {SamVideoRegionAttachment,WireSphericalVideo} from '$lib/composition/types'
 import {measureSphericalSamRegion,getSamStatus} from '$lib/api'
 import {sphericalSemanticSamplePlan,sphericalCentresCameraKeys,sphericalIdentityMeasurements} from '$lib/motion/sphericalSemanticTracking'
 let {region,token,projectId,revision,compositionId,clipId,sourceInSeconds,duration,binding,spherical,canvasAspect=2,initialFov=90,allowFraming=true,disabled=false,onapply}:{region:SamVideoRegionAttachment;token:string;projectId:string;revision:number;compositionId?:string;clipId:string;sourceInSeconds:number;duration:number;binding:unknown;spherical?:WireSphericalVideo;canvasAspect?:number;initialFov?:number;allowFraming?:boolean;disabled?:boolean;onapply:(animation:ReturnType<typeof sphericalCentresCameraKeys>)=>void}=$props()
 let busy=$state(false),error=$state(''),message=$state(''),autoFraming=$state(false),targetFill=$state(.65),reidentify=$state(false)
 let effectiveReidentify=$derived(reidentify||spherical?.input==='dual_fisheye')
 let controller:AbortController|undefined
 $effect(()=>()=>controller?.abort())
 async function apply(){
  if(busy||disabled)return
  const original=binding,snapshot=region,owner=JSON.stringify([token,projectId,revision,compositionId,clipId,sourceInSeconds,duration,region,canvasAspect,initialFov,allowFraming,autoFraming,targetFill,reidentify,spherical]),active=new AbortController();controller=active;busy=true;error='';message=''
  try{
   if(effectiveReidentify){const status=await getSamStatus(token,active.signal);if(!status.available)throw new Error('Локальная модель SAM недоступна для повторной идентификации')}
   if(active.signal.aborted||disabled||binding!==original)return
   const plan=sphericalSemanticSamplePlan(snapshot,sourceInSeconds,duration,effectiveReidentify)
   const result=await measureSphericalSamRegion(token,projectId,{compositionId,clipId,expectedRevision:revision,region:snapshot,sourceTimesUs:plan.times.map(t=>Math.round((sourceInSeconds+t)*1e6)),...(effectiveReidentify?{reidentify:true}:{}),...(spherical?{spherical,parameterStartUs:Math.round(sourceInSeconds*1e6)}:{}),...(autoFraming&&allowFraming?{framing:{aspectRatio:canvasAspect,targetFill,initialFov}}:{})},active.signal)
   if(active.signal.aborted||disabled)return
   if(binding!==original||region!==snapshot||owner!==JSON.stringify([token,projectId,revision,compositionId,clipId,sourceInSeconds,duration,region,canvasAspect,initialFov,allowFraming,autoFraming,targetFill,reidentify,spherical]))throw new Error('Клип или область SAM изменены; повторите анализ')
   if(autoFraming&&allowFraming&&result.centres.some(c=>c.fov===undefined))throw new Error('Сервер не вернул измерения автокадрирования')
   if(autoFraming&&allowFraming&&Math.abs((result.centres[0]?.fov??NaN)-initialFov)>1e-6)throw new Error('Сервер изменил начальное поле зрения; ключи не изменены')
   const measurements=effectiveReidentify?sphericalIdentityMeasurements(result,plan.times):{centres:result.centres,times:plan.times,gaps:0,recovered:0}
   const animation=sphericalCentresCameraKeys(measurements.centres.map(c=>autoFraming&&allowFraming?c:{pan:c.pan,tilt:c.tilt}),measurements.times)
   onapply(animation);message=`Камера следует за областью SAM: ${result.centres.length} измерений. Интервал до 60 секунд.${effectiveReidentify?` Пропусков: ${measurements.gaps}, восстановлений: ${measurements.recovered}. В пропусках камера интерполирует измеренные ключи.`:''}`
  }catch(cause){if(!active.signal.aborted)error=cause instanceof Error?cause.message:String(cause)}finally{if(controller===active){controller=undefined;busy=false}}
 }
</script>
<details><summary>Камера следует за объектом SAM</summary>
 <p>Использует сохранённую область объекта SAM. Центр учитывает шов панорамы и временную шкалу исходника. По умолчанию потеря объекта останавливает анализ; повторный поиск можно включить отдельно.</p>
 {#if allowFraming}<label><input type="checkbox" bind:checked={autoFraming} disabled={disabled||busy}/>Автоматически менять масштаб по размеру объекта</label>{/if}
 <label><input aria-label="Нейронная повторная идентификация SAM" type="checkbox" checked={effectiveReidentify} onchange={event=>reidentify=event.currentTarget.checked} disabled={disabled||busy||spherical?.input==='dual_fisheye'}/>Повторно искать объект после потери</label>
 {#if effectiveReidentify}<p>Для fisheye сначала подготавливает панораму. Ищет объект по всему кадру с учётом согласованных образцов его внешности, собранных до потери. Требуется обновлённый локальный движок SAM. Неоднозначные похожие объекты и перекрытия остаются пропусками; офлайн-анализ до 60 отсчётов может занять десятки минут. Нейронная обработка ограничена 50 минутами, с подготовкой исходника — 2 часами. Анализ можно отменить.</p>{/if}
 {#if autoFraming&&allowFraming}<label>Целевой предел заполнения кадра<input aria-label="Целевой предел заполнения кадра объектом SAM" type="number" min="0.2" max="0.9" step="0.05" bind:value={targetFill} disabled={disabled||busy}/></label><p>Первое поле зрения сохраняется; последующий масштаб плавно следует за объектом, до 30°/с. Целевой размер достигается постепенно. Оценка с запасом для перспективного кадра и любого вращения камеры.</p>{/if}
 <button class="btn ghost sm" disabled={disabled||busy} onclick={()=>void apply()}>Направить камеру на область SAM</button>
 {#if busy}<button class="btn ghost sm" onclick={()=>controller?.abort()}>Отмена</button>{/if}
 {#if error}<p role="alert">{error}</p>{/if}{#if message}<p role="status">{message}</p>{/if}
</details>
