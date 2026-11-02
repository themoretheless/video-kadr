import {writeFileSync} from 'node:fs'
import {buildCompositionWireDocument} from './payload'
import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from './commands'
import { pairImportedVideoAudio } from './importPair'
import { reattachAudio } from './reattachAudio'
import type { AudioClip, Composition, VideoClip } from './types'

function fixture(): Composition {
  let d = registerSource(createComposition({ width: 640, height: 360, fps: 30, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 640, height: 360, hasAudio: true })
  d = addTrack(d, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, clips: [] })
  const clip: VideoClip = { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 4_000_000, transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 0.7, audioPan: -0.25 }
  d = addClip(d, 'video', clip)
  return pairImportedVideoAudio(d, 'picture', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
}
function audioPatch(d: Composition, patch: Partial<AudioClip>): Composition {
  return { ...d, tracks: d.tracks.map(t => t.kind === 'audio' ? { ...t, clips: t.clips.map(c => ({ ...c, ...patch })) } : t) }
}
describe('audio reattachment admission', () => {
  it('works from either member and transfers the source mix without mutating input', () => {
    const d = fixture(), before = JSON.stringify(d)
    for (const id of ['picture', 'sound']) {
      const next = reattachAudio(d, id)
      expect(next.videoId).toBe('picture')
      expect(next.document.tracks[0]!.clips[0]).toMatchObject({ sourceAudioEnabled: true, audioGain: 0.7, audioPan: -0.25 })
      expect(next.document.tracks[1]!.clips).toHaveLength(0)
      expect(next.document.links).toHaveLength(0)
    }
    expect(JSON.stringify(d)).toBe(before)
  })
  it('refuses edits that cannot be represented by embedded audio without deleting anything', () => {
    const base = fixture()
    const cases = [
      { ...base, links: [] },
      audioPatch(base, { locked: true }),
      audioPatch(base, { timelineStartTicks: 1_000_000 }),
      audioPatch(base, { reversed: true }),
      audioPatch(base, { voiceEffect: 'robot' }),
      audioPatch(base, { pitchSemitones: 1 }),
      audioPatch(base, { role: 'music' }),
      { ...base, tracks: base.tracks.map(t => t.kind === 'audio' ? { ...t, muted: true } : t) },
      { ...base, tracks: base.tracks.map(t => t.kind === 'video' ? { ...t, muted: true } : t) },
    ]
    for (const d of cases) {
      const before = JSON.stringify(d)
      expect(() => reattachAudio(d, 'sound')).toThrow()
      expect(JSON.stringify(d)).toBe(before)
    }
  })
})

it('retains source volume keys when rejoining an aligned native audio pair',()=>{
 const keys=[{at:0,value:0.25},{at:1,value:1,ease:[0.42,-0.5,0.58,1] as const}]
 const input=audioPatch(fixture(),{concatGainKeys:keys}),output=reattachAudio(input,'sound')
 expect(output.document.tracks[0]!.clips[0]).toMatchObject({concatGainKeys:keys,sourceAudioEnabled:true})
 expect((output.document.tracks[0]!.clips[0] as VideoClip).concatGainKeys).not.toBe(keys)
})

it('round-trips embedded DSP, overlong fades and normalized volume keys without aliases',()=>{
 const base=fixture();const attached:Composition={...base,links:[],tracks:base.tracks.filter(track=>track.kind==='video').map(track=>({...track,clips:track.clips.map(clip=>({...clip,sourceOutTicks:1000000,sourceAudioEnabled:true,audioGain:0.9,audioPan:0,dspEffects:[{id:'concat.echo',params:{}}],audioFadeInTicks:2000000,audioFadeOutTicks:2000000,concatGainKeys:[{at:0,value:0.25},{at:1,value:1}]}))}))}
 const detached=pairImportedVideoAudio(attached,'picture',{audioTrackId:'soundtrack',audioClipId:'sound',linkId:'new-pair'})
 const sound=detached.tracks.find(track=>track.kind==='audio')!.clips[0] as AudioClip
 expect(sound).toMatchObject({dspEffects:[{id:'concat.echo'}],fadeInTicks:2000000,fadeOutTicks:2000000,concatGainKeys:[{value:0.25},{value:1}]})
 expect(sound.dspEffects).not.toBe((attached.tracks[0]!.clips[0] as VideoClip).dspEffects)
 const rejoined=reattachAudio(detached,'sound').document
 expect(rejoined.tracks[0]!.clips[0]).toMatchObject({sourceAudioEnabled:true,audioFadeInTicks:2000000,audioFadeOutTicks:2000000,dspEffects:sound.dspEffects,concatGainKeys:sound.concatGainKeys})
 if(process.env.CONCAT_LINKED_AUDIO_ROUNDTRIP_OUTPUT)writeFileSync(process.env.CONCAT_LINKED_AUDIO_ROUNDTRIP_OUTPUT,JSON.stringify({attached:buildCompositionWireDocument(attached),detached:buildCompositionWireDocument(detached),reattached:buildCompositionWireDocument(rejoined)}))
 expect(()=>reattachAudio({...detached,audioRoles:{dialogue:{gainDb:-6,muted:false,solo:false}}},'sound')).toThrow('аудиороли')
})

it('retains a selected stream when returning a native linked source-audio pair',()=>{
 const result=reattachAudio(audioPatch(fixture(),{audioStream:2}),'sound')
 expect(result.document.tracks[0]!.clips[0]).toMatchObject({audioStream:2,sourceAudioEnabled:true})
 const paired=pairImportedVideoAudio(result.document,'picture',{audioTrackId:'audio',audioClipId:'again',linkId:'again-link'})
 expect(paired.tracks.find(track=>track.kind==='audio')!.clips[0]).toMatchObject({audioStream:2})
})

it('captures selected-stream linked histories for actual PCM delivery',()=>{
 const base=reattachAudio(fixture(),'sound').document
 const attached:Composition={...base,canvas:{...base.canvas,width:16,height:16},sources:{source:{...base.sources.source!,durationTicks:1000000,width:16,height:16,audioChannels:1}},tracks:base.tracks.filter(track=>track.kind==='video').map(track=>({...track,clips:track.clips.map(clip=>({...clip,sourceOutTicks:1000000,audioStream:2,audioGain:1,audioPan:0,transform:{...clip.transform,width:16,height:16}}))}))}
 const detached=pairImportedVideoAudio(attached,'picture',{audioTrackId:'a',audioClipId:'sound',linkId:'pair'})
 const reattached=reattachAudio(detached,'sound').document
 const edited=audioPatch(detached,{audioStream:1}),editedReattached=reattachAudio(edited,'sound').document
 const defaulted=audioPatch(detached,{audioStream:undefined}),defaultReattached=reattachAudio(defaulted,'sound').document
 const stages=[{name:'attached',document:attached,stream:2},{name:'detached',document:detached,stream:2},{name:'reattached',document:reattached,stream:2},{name:'edited-detached',document:edited,stream:1},{name:'edited-reattached',document:editedReattached,stream:1},{name:'default-reattached',document:defaultReattached,stream:1}]
 for(const stage of stages){const wire=buildCompositionWireDocument(stage.document);const audible=wire.tracks.flatMap<import('./types').WireAudioClip|import('./types').WireVideoClip>(track=>track.kind==='audio'?track.clips:track.kind==='video'?track.clips.filter(clip=>clip.sourceAudioEnabled):[]);expect(audible).toHaveLength(1);expect(audible[0]).toMatchObject(stage.name==='default-reattached'?{}:{audioStream:stage.stream})}
 if(process.env.CONCAT_STREAM_HISTORY_OUTPUT)writeFileSync(process.env.CONCAT_STREAM_HISTORY_OUTPUT,JSON.stringify(stages.map(stage=>({name:stage.name,stream:stage.stream,content:buildCompositionWireDocument(stage.document)}))))
})

it('keeps inactive source keys when returning a linked audio chain to video',()=>{
 const input=audioPatch(fixture(),{dspEffects:[{id:'concat.pitch',params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:-12,ease:[0.42,0,0.58,1]},{at:1,value:12}]}}]})
 const output=reattachAudio(input,'sound').document
 const original=(input.tracks.find(track=>track.kind==='audio')!.clips[0] as AudioClip).dspEffects![0]!
 const effect=(output.tracks[0]!.clips[0] as VideoClip).dspEffects![0]!
 expect(effect.concatSourceKeys).toEqual(original.concatSourceKeys);expect(effect.concatSourceKeys).not.toBe(original.concatSourceKeys);expect(effect.concatSourceKeys!.semitones).not.toBe(original.concatSourceKeys!.semitones);expect(effect.animation).toBeUndefined()
 expect(buildCompositionWireDocument(output).tracks[0]).toMatchObject({clips:[{dspEffects:[{concatSourceKeys:{semitones:[{value:-12},{value:12}]}}]}]})
})
