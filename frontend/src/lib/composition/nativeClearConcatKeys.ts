import type {NativeNestedProject} from '$lib/api'
import type {WireCompositionTrack,WireVideoEffect} from './types'
/** Original clear_all_keys clears clip properties and the first Adjust entry only. */
export function clearNativeConcatKeys(project:NativeNestedProject,scope:string,ids:readonly string[]):NativeNestedProject{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw Error('Таймлайн не найден')
 const selected=new Set(ids),known=new Set(node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)))
 if(ids.some(id=>!known.has(id)))throw Error('Выбранный клип не найден')
 let changed=false
 const tracks=node.content.tracks.map(track=>{
  const clips=track.clips.map(clip=>{
   if(!selected.has(clip.id))return clip
   let next=clip
   const update=(patch:object)=>{if(track.locked)throw Error('Дорожка заблокирована');changed=true;next={...next,...patch}}
   if('transform' in clip&&clip.transform.concatAnimation&&Object.values(clip.transform.concatAnimation.keys).some(run=>run?.length))update({transform:{...clip.transform,concatAnimation:{...clip.transform.concatAnimation,keys:{}}}})
   if('concatGainKeys' in clip&&clip.concatGainKeys?.length)update({concatGainKeys:undefined})
   if('effects' in clip&&clip.effects){
    let found=false,effectsChanged=false
    const clear=(effect:WireVideoEffect):WireVideoEffect=>{
     if(found)return effect
     if(effect.kind==='native_bypass'){const inner=clear(effect.effect);return inner===effect.effect?effect:{...effect,effect:inner}}
     if(effect.kind==='concat_adjust'){
      found=true
      if(Object.values(effect.concatKeys??{}).some(run=>run.length)){effectsChanged=true;return {...effect,concatKeys:undefined}}
     }else if(effect.kind==='concat_bypass'&&effect.link.id==='concat.adjust'){
      found=true
      if(Object.values(effect.link.keys??{}).some(run=>run.length)){effectsChanged=true;return {...effect,link:{...effect.link,keys:{}}}}
     }
     return effect
    }
    const effects=clip.effects.map(clear)
    if(effectsChanged)update({effects})
   }
   return next
  })
  return clips.every((clip,index)=>clip===track.clips[index])?track:{...track,clips} as WireCompositionTrack
 })
 return changed?{...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks}}}}:project
}
