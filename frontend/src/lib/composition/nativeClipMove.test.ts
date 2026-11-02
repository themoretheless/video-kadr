import { expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from './commands'
import { buildCompositionWireDocument } from './payload'
import { nativeClipMoveBatch } from './nativeClipMove'
import type { NativeNestedProject } from '$lib/api'
function fixture(): NativeNestedProject {
  let source = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'media', kind: 'video', durationTicks: 4_000_000, width: 4, height: 4, hasAudio: true })
  source = addTrack(source, { id: 'v', kind: 'video', name: 'V', hidden: false, muted: false, locked: false, clips: [] })
  source = addClip(source, 'v', { id: 'picture', kind: 'video', sourceId: 'media', timelineStartTicks: 1_000_000, sourceInTicks: 0, sourceOutTicks: 2_000_000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
  source = addTrack(source, { id: 'a', kind: 'audio', name: 'A', muted: false, locked: false, clips: [] })
  source = addClip(source, 'a', { id: 'sound', kind: 'audio', sourceId: 'media', timelineStartTicks: 1_500_000, sourceInTicks: 0, sourceOutTicks: 1_000_000, gain: 1 })
  let target = createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' })
  target = addTrack(target, { id: 'target-v', kind: 'video', name: 'V', hidden: false, muted: false, locked: false, clips: [] })
  target = addTrack(target, { id: 'target-a', kind: 'audio', name: 'A', muted: false, locked: false, clips: [] })
  return { schemaVersion: 1, root: 'root', compositions: { root: { content: buildCompositionWireDocument(source), instances: [] }, target: { content: buildCompositionWireDocument(target), instances: [] } } }
}
it('stages a mixed AV batch with common relative offset and same-kind destinations', () => {
  const project = fixture(), before = JSON.stringify(project)
  const moves = nativeClipMoveBatch(project, 'root', 'target', ['sound', 'picture'], 2_000_000, {}, () => 'unused')
  expect(moves.map(move => [move.clipId, move.toTrackId, move.startUs, move.createTrack])).toEqual([['picture', 'target-v', 2_000_000, false], ['sound', 'target-a', 2_500_000, false]])
  expect(JSON.stringify(project)).toBe(before)
  let n = 0
  const fresh = nativeClipMoveBatch(project, 'root', 'target', ['picture', 'sound'], 0, { v: '', a: '' }, () => `new-${n++}`)
  expect(fresh.map(move => [move.toTrackId, move.createTrack])).toEqual([['new-0', true], ['new-1', true]])
})
it('refuses missing selections, locks, invalid positions, target kinds and colliding new IDs', () => {
  const project = fixture()
  for (const ids of [[], ['missing'], ['picture', 'picture']]) expect(() => nativeClipMoveBatch(project, 'root', 'target', ids, 0, {}, () => 'new')).toThrow()
  for (const start of [-1, NaN, 0.5]) expect(() => nativeClipMoveBatch(project, 'root', 'target', ['picture'], start, {}, () => 'new')).toThrow()
  expect(() => nativeClipMoveBatch(project, 'root', 'target', ['picture'], 0, { v: 'target-a' }, () => 'new')).toThrow()
  expect(() => nativeClipMoveBatch(project, 'root', 'target', ['picture', 'sound'], 0, { v: '', a: '' }, () => 'new')).toThrow()
  const locked = { ...project, compositions: { ...project.compositions, target: { ...project.compositions.target!, content: { ...project.compositions.target!.content, tracks: project.compositions.target!.content.tracks.map(track => ({ ...track, locked: true })) } } } }
  expect(() => nativeClipMoveBatch(locked, 'root', 'target', ['picture'], 0, {}, () => 'new')).toThrow()
})
it('resolves one nearest gap for a mixed-track batch and ignores only moving clips in the same timeline',()=>{
 const input=fixture(),target=input.compositions.target.content.tracks,source=input.compositions.root.content.tracks
 const video=source.find(track=>track.kind==='video')!,audio=source.find(track=>track.kind==='audio')!
 const v=target.find(track=>track.kind==='video')!,a=target.find(track=>track.kind==='audio')!
 if(video.kind!=='video'||audio.kind!=='audio'||v.kind!=='video'||a.kind!=='audio')throw Error('tracks')
 Object.assign(v,{clips:[{...video.clips[0],id:'resident-v',placement:{...video.clips[0].placement,timelineStartTick:0,sourceOutTick:3000000}}]})
 Object.assign(a,{clips:[{...audio.clips[0],id:'resident-a',placement:{...audio.clips[0].placement,timelineStartTick:4000000}}]})
 const before=JSON.stringify(input),moves=nativeClipMoveBatch(input,'root','target',['picture','sound'],2000000,{},()=> 'unused')
 expect(moves.map(move=>[move.clipId,move.startUs])).toEqual([['picture',4500000],['sound',5000000]])
 expect(nativeClipMoveBatch(input,'root','root',['picture','sound'],1000000,{},()=> 'unused').map(move=>move.startUs)).toEqual([1000000,1500000]);expect(JSON.stringify(input)).toBe(before)
})
it('places complete moving and stationary transition windows and refuses missing partners',()=>{
 const input=fixture(),source=input.compositions.root.content.tracks[0],target=input.compositions.target.content.tracks[0]
 if(source.kind!=='video'||target.kind!=='video')throw Error('video')
 Object.assign(source,{transitions:[{id:'enter',kind:'dissolve',toClipId:'picture',durationTicks:0,timeline:{startTick:String(500000n*254016n),durationTick:String(1000000n*254016n)}}]})
 Object.assign(target,{clips:[{...source.clips[0],id:'resident',placement:{...source.clips[0].placement,timelineStartTick:0,sourceOutTick:3000000}}]})
 expect(nativeClipMoveBatch(input,'root','target',['picture'],3000000,{},()=> 'unused')[0].startUs).toBe(3500000)
 Object.assign(source,{transitions:[]})
 Object.assign(target,{clips:[{...source.clips[0],id:'resident',placement:{...source.clips[0].placement,timelineStartTick:0,sourceOutTick:2000000}}],transitions:[{id:'leave',kind:'dissolve',fromClipId:'resident',durationTicks:0,timeline:{startTick:String(1750000n*254016n),durationTick:String(750000n*254016n+1n)}}]})
 expect(nativeClipMoveBatch(input,'root','target',['picture'],2000000,{},()=> 'unused')[0].startUs).toBe(2500001)
 Object.assign(source,{transitions:[{id:'between',kind:'dissolve',fromClipId:'picture',toClipId:'partner',durationTicks:0}]})
 expect(()=>nativeClipMoveBatch(input,'root','target',['picture'],2000000,{},()=> 'unused')).toThrow('оба его клипа')
})
