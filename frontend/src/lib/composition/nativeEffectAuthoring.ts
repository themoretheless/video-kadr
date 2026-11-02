import type{NativeNestedProject}from'$lib/api'
import type{WireVideoEffect}from'./types'
import {concatEffectInspectorTracks} from './concatEffectInspectorContext'
/** Immutable visual target resolution, shared by temporary effect documents. */
export function mapNativeClipEffects(base:NativeNestedProject,compositionId:string,trackId:string,clipId:string,update:(effects:readonly WireVideoEffect[])=>readonly WireVideoEffect[]):NativeNestedProject{
 const node=base.compositions[compositionId];if(!node)throw new Error('Композиция не найдена');let found=false
 const eligible=concatEffectInspectorTracks(base,compositionId).find(track=>track.id===trackId);if(!eligible?.clips.some(clip=>clip.id===clipId))throw new Error('Визуальный клип не найден')
 const tracks=node.content.tracks.map(track=>{if(track.id!==trackId||track.kind!=='video'&&track.kind!=='image'&&track.kind!=='text'&&track.kind!=='graphic')return track;if(track.locked)throw new Error('Дорожка заблокирована');const replace=<T extends{id:string;effects?:readonly WireVideoEffect[]}>(clip:T):T=>{if(clip.id!==clipId)return clip;found=true;return{...clip,effects:update(clip.effects??[])}};if(track.kind==='video')return{...track,clips:track.clips.map(replace)};if(track.kind==='text')return{...track,clips:track.clips.map(replace)};if(track.kind==='graphic')return{...track,clips:track.clips.map(replace)};return{...track,clips:track.clips.map(replace)}})
 if(!found)throw new Error('Визуальный клип не найден')
 return{...base,compositions:{...base.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
}
