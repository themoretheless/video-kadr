<script lang="ts">
 import type {WireConcatClipAnimation,WireConcatParamKey} from '$lib/composition/types'
 import Keys from './AudioSourceParamKeys.svelte'
 let {animation,disabled=false,onchange}:{animation:WireConcatClipAnimation;disabled?:boolean;onchange:(property:string,keys:readonly WireConcatParamKey[])=>void}=$props()
 const properties=['scale','offsetX','offsetY','rotation','opacity'] as const
 const names={scale:'Масштаб',offsetX:'Смещение X',offsetY:'Смещение Y',rotation:'Вращение',opacity:'Непрозрачность'}
 function rest(property:typeof properties[number]){return property==='scale'?animation.rest.scaleX:property==='offsetX'?animation.rest.x:property==='offsetY'?animation.rest.y:property==='rotation'?animation.rest.rotationDegrees:animation.rest.opacity}
</script>
<fieldset {disabled}>
 <legend>Анимация клипа Concat</legend>
 {#each properties as property}
  <Keys keys={animation.keys[property]??[]} rest={rest(property)} label={names[property]} description="Управляют изображением. Положение — доля длительности клипа; easing применяется при подходе к ключу." onchange={keys=>onchange(property,keys)}/>
 {/each}
</fieldset>
