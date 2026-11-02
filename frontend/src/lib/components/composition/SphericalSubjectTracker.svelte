<script lang="ts">
  import {tick} from 'svelte'
  import {sampleBrowserVideoLumaFrames,type BrowserTrackingFrames} from '$lib/motion/browserFrameSampler'
  import {trackSphericalSubject} from '$lib/motion/sphericalTracking'
  let {url,sourceInSeconds,duration,binding,disabled=false,onapply}:{url:string;sourceInSeconds:number;duration:number;binding:unknown;disabled?:boolean;onapply:(animation:Pick<ReturnType<typeof trackSphericalSubject>,'pan'|'tilt'>)=>void}=$props()
  let busy=$state(false),error=$state(''),message=$state('')
  let prepared=$state<BrowserTrackingFrames|null>(null),point=$state<readonly [number,number]|null>(null)
  let canvas=$state<HTMLCanvasElement>(),controller:AbortController|undefined
  let preparedBinding:unknown,times:number[]=[]
  $effect(()=>()=>controller?.abort())
  async function prepare(){
    if(busy||disabled)return;controller?.abort();const active=new AbortController();controller=active;busy=true;error='';message='';prepared=null;point=null
    const original=binding
    try{
      const end=Math.min(duration,60);if(!Number.isFinite(end)||end<.2)throw new Error('Слишком короткий клип')
      const selected=Array.from({length:Math.min(300,Math.ceil(end*5))},(_,i)=>i/5)
      const frames=await sampleBrowserVideoLumaFrames(url,selected.map(t=>sourceInSeconds+t),{maxWidth:256,maxHeight:128,timeoutMs:120000,signal:active.signal})
      if(active.signal.aborted||binding!==original)throw new Error('Клип изменён во время подготовки')
      times=selected;preparedBinding=original;prepared=frames;await tick()
      if(canvas){canvas.width=frames.width;canvas.height=frames.height;const data=new Uint8ClampedArray(frames.width*frames.height*4);frames.frames[0]!.data.forEach((v,i)=>{data.set([v,v,v,255],i*4)});canvas.getContext('2d')?.putImageData(new ImageData(data,frames.width,frames.height),0,0)}
      message='Выберите контрастную точку объекта на панораме. Анализ: первые 60 секунд, 5 кадров/с.'
    }catch(cause){if(!active.signal.aborted)error=cause instanceof Error?cause.message:String(cause)}finally{if(controller===active){controller=undefined;busy=false}}
  }
  function choose(event:MouseEvent){if(!canvas||!prepared)return;const rect=canvas.getBoundingClientRect();point=event.detail===0?[.5,.5]:[Math.max(0,Math.min(.999,(event.clientX-rect.left)/rect.width)),Math.max(0,Math.min(.999,(event.clientY-rect.top)/rect.height))]}
  function apply(){if(!prepared||!point||disabled||busy)return;error='';try{if(binding!==preparedBinding)throw new Error('Клип изменён; подготовьте кадры заново');const animation=trackSphericalSubject(prepared.frames,times,point);onapply({pan:animation.pan,tilt:animation.tilt});message=`Ключи камеры добавлены: ${prepared.frames.length} кадров. Восстановлений после перекрытия: ${animation.recoveredGaps.length}. Между потерей и восстановлением камера движется по интерполяции.`;prepared=null;point=null}catch(cause){error=cause instanceof Error?cause.message:String(cause)}}
</script>
<details><summary>Следовать за выбранной точкой на панораме</summary>
  <p>Трекинг контрастного фрагмента, с поиском повторно появившегося фрагмента после перекрытия до 2 секунд. Одинаковые фрагменты неоднозначны; распознавание объектов не используется.</p>
  <button class="btn ghost sm" disabled={disabled||busy} onclick={()=>void prepare()}>Подготовить панораму для трекинга</button>
  {#if busy}<button class="btn ghost sm" onclick={()=>controller?.abort()}>Отмена</button>{/if}
  {#if prepared}<button class="preview" aria-label="Выбрать точку на панораме" onclick={choose}><canvas bind:this={canvas}></canvas></button>{/if}
  {#if point}<p>Точка: {point[0].toFixed(3)}, {point[1].toFixed(3)}</p><button class="btn primary sm" disabled={disabled||busy} onclick={apply}>Отследить и направить камеру</button>{/if}
  {#if error}<p role="alert">{error}</p>{/if}{#if message}<p role="status">{message}</p>{/if}
</details>
<style>.preview{display:block;max-width:100%;padding:0}canvas{display:block;max-width:100%;image-rendering:auto}</style>
