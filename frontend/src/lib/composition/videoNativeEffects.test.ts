import {writeFileSync} from 'node:fs'
import {expect,it} from 'vitest'
import {createComposition,duplicateClip,splitClip,trimClip} from './commands'
import {normalizeComposition} from './validation'
import {buildCompositionRenderRequest} from './payload'
import type {Composition,VideoClip,WireVideoEffect} from './types'
const effect:WireVideoEffect={kind:'concat_exposure',parameters:{},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,-0.5,0.58,1]}]}
function fixture(effects:readonly WireVideoEffect[]=[effect]):Composition{return {...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:false}},tracks:[{id:'v',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[{id:'clip',kind:'video',sourceId:'source',timelineStartTicks:1000000,sourceInTicks:0,sourceOutTicks:1000000,speed:1,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1,effects}],transitions:[]}]}}
function clips(c:Composition):readonly VideoClip[]{const track=c.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips}
it('serializes original native video keys unchanged through persistence and render payload',()=>{
 const c=normalizeComposition(JSON.parse(JSON.stringify(fixture())));const wire=buildCompositionRenderRequest(c).composition;expect(wire.tracks[0]).toMatchObject({clips:[{effects:[effect]}]})
 if(process.env.CONCAT_FLAT_VIDEO_OUTPUT)writeFileSync(process.env.CONCAT_FLAT_VIDEO_OUTPUT,JSON.stringify(wire))
 expect(()=>buildCompositionRenderRequest(fixture([{kind:'unknown_effect'} as unknown as WireVideoEffect]))).toThrow('Неподдерживаемые clip effects')
})
it('retains declared Concat families and chain ordering alongside legacy styles',()=>{
 for(const kind of ['concat_bloom_pulse','concat_chromatic_aberration','concat_film_grain','concat_fisheye','concat_glow','concat_halftone','concat_motion_blur','concat_zoom_blur'] as const){const e:WireVideoEffect={kind,parameters:{}};expect(buildCompositionRenderRequest(fixture([e])).composition.tracks[0]).toMatchObject({clips:[{effects:[e]}]})}
 const c=fixture([effect,{kind:'concat_contrast',parameters:{amount:20}}]);const track=c.tracks[0]!;if(track.kind!=='video')throw new Error('track');const styled={...c,tracks:[{...track,clips:track.clips.map(clip=>({...clip,videoEffects:[{preset:'blur' as const,intensity:0.2}]}))}]};expect(buildCompositionRenderRequest(styled).composition.tracks[0]).toMatchObject({clips:[{effects:[{kind:'style',preset:'blur',intensity:0.2},effect,{kind:'concat_contrast',parameters:{amount:20}}]}]})
})
it('deep-copies effects and rebases native tick curves on split and trim',()=>{
 const animated:WireVideoEffect={kind:'concat_contrast',parameters:{},animation:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:100}]}}}
 const c=fixture([effect,animated]);const dup=duplicateClip(c,'clip',{id:'copy'});expect(clips(dup)[1]!.effects).toEqual([effect,animated]);expect(clips(dup)[1]!.effects).not.toBe(clips(c)[0]!.effects)
 const right=clips(splitClip(c,'clip',1500000,'right'))[1]!;expect(right.effects?.[0]).toMatchObject({concatKeys:[{at:0,value:1.25},{at:1,value:4}]});expect(right.effects?.[1]).toMatchObject({animation:{track:{keyframes:[{tick:-500000,value:0},{tick:500000,value:100}]}}})
 const trimmed=clips(trimClip(c,'clip',1250000,2000000))[0]!;expect(trimmed.effects?.[0]).toMatchObject({concatKeys:[{at:0},{at:1,value:4}]});expect(trimmed.effects?.[1]).toMatchObject({animation:{track:{keyframes:[{tick:-250000,value:0},{tick:750000,value:100}]}}})
})

it('captures the original source-window split payload',()=>{const c=splitClip(fixture(),'clip',1500000,'right');const wire=buildCompositionRenderRequest(c).composition;expect(clips(c)[1]!.effects?.[0]).toMatchObject({concatKeys:[{at:0,value:1.25},{at:1,value:4}]});if(process.env.CONCAT_SPLIT_VIDEO_OUTPUT)writeFileSync(process.env.CONCAT_SPLIT_VIDEO_OUTPUT,JSON.stringify(wire))})

it('captures an overshoot-derived split without restarting or clamping its boundary',()=>{
 const overshoot:WireVideoEffect={kind:'concat_exposure',parameters:{},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,3,0.58,3]}]}
 const c=splitClip(fixture([overshoot]),'clip',1500000,'right');expect(clips(c)[1]!.effects?.[0]).toMatchObject({concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]});if(process.env.CONCAT_DERIVED_VIDEO_OUTPUT)writeFileSync(process.env.CONCAT_DERIVED_VIDEO_OUTPUT,JSON.stringify(buildCompositionRenderRequest(c).composition))
})

it('preserves embedded DSP and independent routing through payload and native automation edits',()=>{
 const original=fixture([]),track=original.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const dspEffects=[{id:'amplify',animation:{gain:{mode:'keyframes' as const,track:{timeBase:1000000,interpolation:'linear' as const,keyframes:[{tick:0,value:-6},{tick:1000000,value:0}]}}}}]
 const c={...original,tracks:[{...track,clips:track.clips.map(clip=>({...clip,dspEffects,independentAudio:true}))}]};expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{dspEffects,independentAudio:true}]})
 const right=clips(splitClip(c,'clip',1500000,'right'))[1]!;expect(right.independentAudio).toBe(true);expect(right.dspEffects?.[0]).toMatchObject({animation:{gain:{track:{keyframes:[{tick:-500000,value:-6},{tick:500000,value:0}]}}}})
})

it('keeps embedded audio fades through payload and assigns split fades to outer edges',()=>{
 const original=fixture([]),track=original.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const c={...original,tracks:[{...track,clips:track.clips.map(clip=>({...clip,audioFadeInTicks:750000,audioFadeOutTicks:750000}))}]}
 expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{audioFadeInTicks:750000,audioFadeOutTicks:750000}]})
 const parts=clips(splitClip(c,'clip',1500000,'right'))
 expect(parts[0]).toMatchObject({audioFadeInTicks:750000,audioFadeOutTicks:0})
 expect(parts[1]).toMatchObject({audioFadeInTicks:0,audioFadeOutTicks:750000})
 expect(clips(trimClip(c,'clip',1250000,1750000))[0]).toMatchObject({audioFadeInTicks:750000,audioFadeOutTicks:750000})
 expect(()=>buildCompositionRenderRequest({...c,tracks:[{...track,clips:track.clips.map(clip=>({...clip,audioFadeInTicks:-1}))}]})).toThrow()
})

it('retains overlong embedded fades in persisted render payloads',()=>{
 const original=fixture([]),track=original.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const c={...original,tracks:[{...track,clips:track.clips.map(clip=>({...clip,audioFadeInTicks:2000000,audioFadeOutTicks:2000000}))}]}
 expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{audioFadeInTicks:2000000,audioFadeOutTicks:2000000}]})
 const parts=clips(splitClip(c,'clip',1500000,'tail'));expect(parts[0]).toMatchObject({audioFadeInTicks:2000000,audioFadeOutTicks:0});expect(parts[1]).toMatchObject({audioFadeInTicks:0,audioFadeOutTicks:2000000})
})
