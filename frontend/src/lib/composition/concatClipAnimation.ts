/** Persisted original Concat visual-property clocks, separate from native curves. */
export function validConcatClipAnimation(value:unknown):boolean{
 const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v)
 const finite=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)
 if(!object(value)||!Number.isSafeInteger(value.durationTicks)||Number(value.durationTicks)<=0||!object(value.keys)||!object(value.rest))return false
 if(!['scaleX','scaleY','x','y','rotationDegrees','opacity'].every(key=>finite((value.rest as Record<string,unknown>)[key])))return false
 if(!Array.isArray(value.stretch)||value.stretch.length!==2||!value.stretch.every(finite)||!Array.isArray(value.canvas)||value.canvas.length!==2||!value.canvas.every(v=>finite(v)&&v>0))return false
 let total=0
 for(const [property,run]of Object.entries(value.keys)){
  if(!['scale','offsetX','offsetY','rotation','opacity'].includes(property)||!Array.isArray(run)||run.length>4096)return false
  total+=run.length
  for(const key of run){
   if(!object(key)||!finite(key.at)||key.at<0||key.at>1||!finite(key.value)||key.derived!==undefined&&typeof key.derived!=='boolean')return false
   if(key.ease!==undefined){
    if(typeof key.ease==='string'){if(!['linear','in','out','inOut'].includes(key.ease))return false}
    else if(!Array.isArray(key.ease)||key.ease.length!==4||!key.ease.every(finite)||key.ease[0]<0||key.ease[0]>1||key.ease[2]<0||key.ease[2]>1)return false
   }
  }
 }
 return total<=8192
}
