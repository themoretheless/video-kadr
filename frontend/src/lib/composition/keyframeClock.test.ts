import { expect, it } from 'vitest'
import { sampleAnimatableValue, cloneAnimatableValue, shiftAnimatableValue, sliceAnimatableValue, splitAnimatableValue } from './keyframes'
import { canonicalSampleClock, sampleClockTick, validSampleClock } from './keyframeClock'
import type { CompositionAnimatableValue } from './types'
function keyed(): Extract<CompositionAnimatableValue,{mode:'keyframes'}> { return {mode:'keyframes',track:{timeBase:24,interpolation:'linear',sampleClock:{rateNum:1,rateDen:1,offsetNum:100000},keyframes:[{tick:0,value:0},{tick:24,value:24}]}} }
it('samples after the rational offset and preserves the phase missed by rounded key movement',()=>{
  const value=keyed();expect(sampleAnimatableValue(value,10000)).toBe(3)
  expect(sampleAnimatableValue({...value,track:{...value.track,sampleClock:undefined,keyframes:[{tick:-2,value:0},{tick:22,value:24}]}},10000)).toBe(2)
  expect(canonicalSampleClock({rateNum:2,rateDen:2,offsetNum:200000})).toEqual(value.track.sampleClock)
  const cloned=cloneAnimatableValue(value);expect(cloned).toEqual(value);if(cloned.mode!=='keyframes')throw new Error('fixture');expect(cloned.track.sampleClock).not.toBe(value.track.sampleClock)
})
it('folds extrapolation in authored coordinates without applying the clock twice',()=>{
  const value=keyed(),loop={...value,track:{...value.track,extrapolation:{before:'loop' as const,after:'ping_pong' as const}}}
  expect(sampleAnimatableValue(loop,1100000)).toBe(19)
  expect(sampleAnimatableValue(loop,-300000)).toBe(19)
  const negative={...value,track:{...value.track,sampleClock:{rateNum:1,rateDen:1,offsetNum:-100000},extrapolation:{before:'continue' as const,after:'hold' as const}}}
  expect(sampleAnimatableValue(negative,37500)).toBe(-2)
})
it('keeps fractional hold rates and rejects invalid numeric domains',()=>{
  const value=keyed(),track={...value.track,sampleClock:{rateNum:64429,rateDen:15625,offsetNum:0}}
  expect(sampleClockTick(track,300000,1000000)).toBe(30)
  for(const clock of [{rateNum:0,rateDen:1,offsetNum:0},{rateNum:1,rateDen:0,offsetNum:0},{rateNum:1,rateDen:1,offsetNum:0,path:'bad'}])expect(validSampleClock(clock)).toBe(false)
  expect(()=>sampleClockTick({...track,timeBase:100000001},0,1000000)).toThrow()
})

it('keeps scalar clock metadata through the actual native payload boundary',async()=>{
  const {addClip,addTrack,createComposition,registerSource}=await import('./commands'),{buildCompositionWireDocument}=await import('./payload')
  let c=registerSource(createComposition({width:16,height:16,fps:25,backgroundColor:'#000000'}),{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:false})
  c=addTrack(c,{id:'v',kind:'video',name:'V',hidden:false,muted:false,locked:false,clips:[]})
  const value=keyed();c=addClip(c,'v',{id:'clip',kind:'video',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1,animation:{x:value}})
  const wire=buildCompositionWireDocument(c),video=wire.tracks.find(track=>track.kind==='video');if(video?.kind!=='video')throw new Error('fixture');const x=video.clips[0]!.transform.x
  expect(x).toMatchObject({mode:'keyframes',track:{sampleClock:value.track.sampleClock}})
  if(x.mode!=='keyframes')throw new Error('fixture');expect(x.track.sampleClock).not.toBe(value.track.sampleClock)
  const {packColorAnimation}=await import('./colorAnimation')
  expect(()=>packColorAnimation({mode:'keyframes',track:{...value.track,keyframes:[{tick:0,value:'#ff0000'}]}})).toThrow('часы цвета')
})

it('split, trim and movement retain fractional phase and the original extrapolated curve', () => {
  const initial = keyed()
  const value = { ...initial, track: { ...initial.track, extrapolation: { before: 'loop' as const, after: 'ping_pong' as const } } }
  const original = structuredClone(value)
  const tail = splitAnimatableValue(value, 123457, true)
  const trimmed = sliceAnimatableValue(value, 123457, 765432, 1000000)
  const moved = shiftAnimatableValue(value, 123457)
  for (const time of [-200000, 0, 1, 10000, 500000, 1200000]) {
    expect(sampleAnimatableValue(tail, time)).toBe(sampleAnimatableValue(value, time + 123457))
    expect(sampleAnimatableValue(trimmed, time)).toBe(sampleAnimatableValue(value, time + 123457))
    expect(sampleAnimatableValue(moved, time + 123457)).toBe(sampleAnimatableValue(value, time))
  }
  expect(value).toEqual(original)
  if (tail.mode !== 'keyframes') throw new Error('Expected keyed tail')
  expect(tail.track.keyframes).toEqual(value.track.keyframes)
})

it('inverts authored positions and inserts at the clock-mapped cursor including signed keys', async () => {
  const { keyframeLocalTime, keyframeAuthoredTick } = await import('./keyframeClock')
  const { keyframeTickAtLocalTime, upsertKeyframe } = await import('./keyframes')
  const value = keyed()
  const track = { ...value.track, sampleClock: { rateNum: 2, rateDen: 1, offsetNum: -100000 } }
  expect(keyframeLocalTime(track, 0)).toBe(50000)
  expect(keyframeLocalTime(track, 24)).toBe(550000)
  for (const tick of [-24, 0, 1, 12, 24]) expect(keyframeAuthoredTick(track, keyframeLocalTime(track, tick))).toBe(tick)
  const tick = keyframeTickAtLocalTime(0, track.timeBase, 1000000, track)
  expect(tick).toBe(-2)
  const edited = upsertKeyframe({ ...value, track }, tick, 99, 0)
  expect(sampleAnimatableValue(edited, 0)).toBe(99)
})
