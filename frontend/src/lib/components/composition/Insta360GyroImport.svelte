<script lang="ts">
 import {analyzeInsta360Gyro,inspectInsta360Source,type Insta360GyroAnalysis} from '$lib/api'
 import type {WireSphericalVideo} from '$lib/composition/types'
 import {parseDriftCorrection} from '$lib/motion/gyroOptions'
 type Imported=Pick<WireSphericalVideo,'orientation'|'orientationOffsetSeconds'|'sourceFrameClock'|'rollingShutter'>
 let {sourceId,token,startSeconds,durationSeconds,binding,allowRollingShutter=false,disabled=false,onapply}:{sourceId:string;token:string;startSeconds:number;durationSeconds:number;binding:unknown;allowRollingShutter?:boolean;disabled?:boolean;onapply:(parameters:Imported)=>void}=$props()
 let busy=$state(false),error=$state(''),result=$state<Insta360GyroAnalysis|null>(null),mounting=$state(''),offset=$state(0)
 let visualDrift=$state(false),driftText=$state(''),rolling=$state(false),readoutMs=$state(''),scan0=$state<'top_to_bottom'|'bottom_to_top'>('top_to_bottom'),scan1=$state<'top_to_bottom'|'bottom_to_top'>('top_to_bottom')
 let controller:AbortController|undefined
 $effect(()=>()=>controller?.abort())
 const exposureQualified=(sync:string)=>sync==='exposure_midpoint_linear'||sync==='exposure_midpoint_trim_map'
 const scope=()=>JSON.stringify([sourceId,token,startSeconds,durationSeconds,allowRollingShutter,visualDrift,driftText,rolling,readoutMs,scan0,scan1,mounting,offset])
 async function run(){
  if(disabled||busy)return
  const captured=binding,owner=scope(),active=new AbortController();controller=active;busy=true;error='';result=null
  try{
   let cameraFromSensor:readonly [number,number,number,number]|undefined
   if(mounting.trim()){
    const parsed:unknown=JSON.parse(mounting)
    if(!Array.isArray(parsed)||parsed.length!==4||parsed.some(v=>typeof v!=='number'||!Number.isFinite(v))||Math.abs(Math.hypot(...parsed)-1)>.001)throw new Error('Нужен единичный кватернион [w,x,y,z]')
    cameraFromSensor=parsed as [number,number,number,number]
   }
   if(durationSeconds>600)throw new Error('Для gyro выберите клип не длиннее 10 минут')
   if(driftText.length>8192)throw new Error('Калибровка дрейфа превышает 8 КБ')
   const driftCorrection=driftText.trim()?parseDriftCorrection(JSON.parse(driftText),durationSeconds):undefined
   const useRolling=rolling&&allowRollingShutter
   const [analysis,metadata]=await Promise.all([
    analyzeInsta360Gyro(sourceId,token,{startSeconds,durationSeconds,sampleRateHz:Math.min(60,Math.floor(19998/durationSeconds)),cameraFromSensor,timeOffsetSeconds:offset,visualDriftCorrection:visualDrift,driftCorrection,orientationToleranceDegrees:.005,motionPaddingSeconds:useRolling?.1:0},active.signal),
    useRolling&&!readoutMs.trim()?inspectInsta360Source(sourceId,token,active.signal):Promise.resolve(null),
   ])
   if(active.signal.aborted)return
   if(disabled)throw new Error('Редактирование клипа заблокировано')
   if(binding!==captured||owner!==scope())throw new Error('Клип или настройки изменились во время анализа; повторите импорт')
   result=analysis
   if(analysis?.status==='orientation_ready'&&analysis.orientation.length){
    let rollingShutter:WireSphericalVideo['rollingShutter']
    if(useRolling){
     if(!analysis.sourceFrameClock||!exposureQualified(analysis.synchronization)||!analysis.captureClock?.length)throw new Error('Для rolling shutter нужны подтверждённые часы кадров и экспозиции')
     const readout=readoutMs.trim()?Number(readoutMs):metadata?.rollingShutterTime
     if(typeof readout!=='number'||!Number.isFinite(readout)||readout<=0||readout>100)throw new Error('Укажите время считывания сенсора от 0 до 100 мс')
     rollingShutter={readoutSeconds:readout/1000,referenceRow:.5,scanDirections:[scan0,scan1],captureClock:analysis.captureClock}
    }
    onapply({orientation:analysis.orientation,orientationOffsetSeconds:analysis.orientationOffsetSeconds??0,sourceFrameClock:analysis.sourceFrameClock??undefined,rollingShutter})
   }
  }catch(cause){if(!active.signal.aborted)error=cause instanceof Error?cause.message:String(cause)}finally{if(controller===active){controller=undefined;busy=false}}
 }
</script>
<details><summary>Встроенный gyro Insta360</summary>
 <p>Чтение датчиков и привязка к времени захвата. Оси определяются автоматически для поддерживаемых профилей камеры. Для других вариантов укажите проверенную калибровку датчика относительно камеры.</p>
 <label>Датчик → камера, [w,x,y,z]<input aria-label="Gyro camera from sensor" bind:value={mounting} placeholder="Автоматически из профиля" disabled={disabled||busy}/></label>
 <label>Поправка синхронизации, с<input aria-label="Gyro capture clock offset" type="number" min="-60" max="60" step="0.001" bind:value={offset} disabled={disabled||busy}/></label>
 <label><input type="checkbox" bind:checked={visualDrift} disabled={disabled||busy}/>Коррекция дрейфа по неподвижному фону видео</label>
 <p>Движение камеры сохраняется. Поправка направления применяется только при согласованных измерениях фона; недостаток текстуры выводится как предупреждение.</p>
 {#if allowRollingShutter}
  <label><input type="checkbox" bind:checked={rolling} disabled={disabled||busy}/>Коррекция rolling shutter</label>
  {#if rolling}
   <label>Считывание сенсора, мс<input aria-label="Sensor readout milliseconds" type="text" inputmode="decimal" bind:value={readoutMs} placeholder="Из метаданных камеры" disabled={disabled||busy}/></label>
   {#each [0,1] as lens (lens)}<label>Сканирование объектива {lens+1}<select aria-label={`Sensor ${lens} scan direction`} value={lens===0?scan0:scan1} disabled={disabled||busy} onchange={event=>{const v=event.currentTarget.value as typeof scan0;if(lens===0)scan0=v;else scan1=v}}><option value="top_to_bottom">Сверху вниз</option><option value="bottom_to_top">Снизу вверх</option></select></label>{/each}
  {/if}
 {/if}
 <details><summary>Калибровка дрейфа по измерениям</summary><p>JSON: biasRadiansPerSecond в осях датчика; stationaryIntervals — явно неподвижные интервалы; headingAnchors — измеренное направление в градусах. Медленное панорамирование не считается неподвижностью.</p><textarea aria-label="Gyro drift observations" bind:value={driftText} maxlength="8192" disabled={disabled||busy}></textarea></details>
 <button class="btn ghost sm" disabled={disabled||busy} onclick={()=>void run()}>{mounting.trim()?'Импортировать встроенную ориентацию':'Импортировать встроенный gyro'}</button>
 {#if busy}<button class="btn ghost sm" onclick={()=>controller?.abort()}>Отмена</button>{/if}
 {#if result}<p role="status">Прочитано отсчётов: {result.inputSamples}. {result.status==='orientation_ready'?'Ориентация рассчитана.':'Показания извлечены; нужна калибровка осей камеры.'} {(result.mountingSource==='embedded_v6_145'||result.mountingSource==='embedded_legacy_145')?'Оси взяты из профиля камеры.':''} {result.synchronization==='exposure_midpoint_trim_map'?'Учтены центр экспозиции и временная карта обрезанного исходника.':result.synchronization==='exposure_midpoint_linear'?'Учтён центр экспозиции для обычной записи.':'Привязка по времени захвата без компенсации экспозиции.'}</p>{/if}
 {#if result?.driftCorrection?.visualFrames}<p role="status">Измерений дрейфа по фону: {result.driftCorrection.visualPairs}; отклонено: {result.driftCorrection.visualRejectedPairs}. Поправка направления: {result.driftCorrection.correctedHeadingDegrees.toFixed(3)}°.</p>{/if}
 {#each result?.driftCorrection?.warnings??[] as warning,i (i)}<p role="status">{warning}</p>{/each}
 {#if result?.gravityAlignment?.warning}<p role="status">{result.gravityAlignment.status==='unavailable_dynamic_start'?'В начале участка сильное ускорение: абсолютный горизонт не подтверждён. Для его выравнивания выберите более стабильное начало.':result.gravityAlignment.status==='estimated_observed_preroll'?'Горизонт оценён по короткому участку до начала клипа; точное выравнивание не подтверждено.':result.gravityAlignment.status==='sensor_axes_required'?'Для выравнивания горизонта нужна калибровка осей датчика.':result.gravityAlignment.warning}</p>{/if}
 {#if result?.timingWarning}<p role="status">{exposureQualified(result.synchronization)?'За пределами видеоклипа временная шкала расширена; используются реальные gyro-данные.':'Точное время экспозиции не подтверждено; используется приблизительная привязка по времени захвата. Проверьте синхронизацию и при необходимости задайте поправку.'}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
