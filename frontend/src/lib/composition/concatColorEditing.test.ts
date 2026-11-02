import {expect,it} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-color-edit.json'
import type {NativeNestedProject} from '$lib/api'
import type {WireAnimatableValue,WireVideoClip,WireVideoEffect} from './types'
import {rebaseNativeEffects} from './nativeEffects'
import {sampleAnimatableValue} from './keyframes'
import {splitNativeClips} from './nativeClipSplit'
import {trimNativeClipStarts} from './nativeClipTrim'
import {duplicateNativeClips} from './nativeClipDuplicate'
function curves(effect:WireVideoEffect):WireAnimatableValue[]{
 if(effect.kind==='concat_curves'||effect.kind==='concat_sharpen'||effect.kind==='concat_vignette'||effect.kind==='concat_contrast_pop'||effect.kind==='concat_bleach_bypass'||effect.kind==='concat_super8'||effect.kind==='concat_retro_cartoon'||effect.kind==='concat_marquee'||effect.kind==='concat_mono'||effect.kind==='concat_midnight'||effect.kind==='concat_vhs'||effect.kind==='concat_falling_dust'||effect.kind==='concat_film'||effect.kind==='concat_digicam'||effect.kind==='concat_autumn'||effect.kind==='concat_neon'||effect.kind==='concat_pink_fuzz'||effect.kind==='concat_night'||effect.kind==='concat_spooky'||effect.kind==='concat_cool'||effect.kind==='concat_warm'||effect.kind==='concat_teal_orange'||effect.kind==='concat_noir'||effect.kind==='concat_dreamy'||effect.kind==='concat_high_contrast_mono'||effect.kind==='concat_landscape_pop'||effect.kind==='concat_natural'||effect.kind==='concat_retro_70s'||effect.kind==='concat_silver'||effect.kind==='concat_tropical'||effect.kind==='concat_vaporwave'||effect.kind==='concat_winter_cool'||effect.kind==='concat_classic_chrome'||effect.kind==='concat_cyberpunk'||effect.kind==='concat_delicious'||effect.kind==='concat_rosy'||effect.kind==='concat_sci_fi'||effect.kind==='concat_clean'||effect.kind==='concat_matcha'||effect.kind==='concat_sage'||effect.kind==='concat_ivory'||effect.kind==='concat_milk'||effect.kind==='concat_bronze'||effect.kind==='concat_cool_blue'||effect.kind==='concat_fresh'||effect.kind==='concat_vivid'||effect.kind==='concat_soft_light'||effect.kind==='concat_moody'||effect.kind==='concat_rain'||effect.kind==='concat_umber'||effect.kind==='concat_fair'||effect.kind==='concat_peach'||effect.kind==='concat_polaroid'||effect.kind==='concat_vintage'||effect.kind==='concat_cloudless_sky'||effect.kind==='concat_lagoon'||effect.kind==='concat_golden_hour'||effect.kind==='concat_daydream')return[]
 if(effect.kind==='concat_color_wheels'||effect.kind==='concat_adjust')return Object.values(effect.animation!)as WireAnimatableValue[]
 if(effect.kind==='concat_white_balance')return[effect.animation!.temperature!,effect.animation!.tint!]
 if(effect.kind==='concat_vibrance')return[effect.animation!.amount!,effect.animation!.intensity!]
 if(effect.kind==='concat_contrast'||effect.kind==='concat_exposure'||effect.kind==='concat_saturation')return[effect.animation!]
 throw new Error('Concat color effect required')
}
it('retains every Concat curve and independent data through exact signed rebasing',()=>{
 for(const c of fixture.cases){
  const project=c.original as unknown as NativeNestedProject,clip=project.compositions.root!.content.tracks[0]!.clips[0]!as WireVideoClip
  const before=JSON.stringify(clip.effects),shifted=rebaseNativeEffects(clip.effects,-1250000)!
  const old=clip.effects.flatMap(curves),next=shifted.flatMap(curves)
  expect(next).toHaveLength(old.length)
  for(let i=0;i<old.length;i++){
   const original=old[i]!,rebased=next[i]!
   if(original.mode!=='keyframes'||rebased.mode!=='keyframes')throw new Error('keyed fixture required')
   expect(rebased).not.toBe(original);expect(rebased.track).toMatchObject({timeBase:original.track.timeBase,interpolation:'ease_in_out_cubic',extrapolation:{before:'hold',after:'hold'}})
   expect(rebased.track.keyframes).toEqual(original.track.keyframes.map(point=>({...point,tick:point.tick-1250000*original.track.timeBase/1000000})))
   for(const time of [0,1,250000,1000000,2000000])expect(sampleAnimatableValue(next[i]!,time)).toBeCloseTo(sampleAnimatableValue(old[i]!,time+1250000),12)
  }
  if(next.length===2)expect(next[0]).not.toBe(next[1]);expect(JSON.stringify(clip.effects)).toBe(before)
 }
})
it('refuses locked edits without mutating retained Concat animation or parameters',()=>{
 for(const c of fixture.cases){
  const project=structuredClone(c.original)as unknown as NativeNestedProject,node=project.compositions.root!
  project.compositions.root={...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:true}))}}
  const before=JSON.stringify(project)
  expect(()=>splitNativeClips(project,'root',['clip'],2250000,()=> 'tail')).toThrow('заблокирована')
  expect(()=>trimNativeClipStarts(project,'root',['clip'],750000)).toThrow('заблокирована')
  expect(()=>duplicateNativeClips(project,'root',['clip'],()=> 'copy',6000000)).toThrow('заблокирована')
  expect(JSON.stringify(project)).toBe(before)
 }
})
