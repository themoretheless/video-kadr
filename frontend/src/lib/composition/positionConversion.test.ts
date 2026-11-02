import { writeFileSync } from 'node:fs'
import { expect,it } from 'vitest'
import { unpackPositionConversion,unpackConversionTrack,packConversionTrack,packPositionConversion } from './positionConversion'
import { sampleAnimatableValue } from './keyframes'
import type { CompositionAnimatableValue,CompositionInterpolation } from './types'
it('packs clocks and legacy cubic segments without changing samples or authored records', () => {
  for (const interpolation of ['linear','hold','ease_in','ease_out','ease_in_out','ease_in_out_cubic',{ kind: 'bezier',p1: [0.2,-0.2],p2: [0.8,1.2] }] as CompositionInterpolation[]) {
    const source: CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation,keyframes: [{ tick: 0,value: 10 },{ tick: 1000,value: 30 }] } }
    const before = JSON.stringify(source)
    const packed = packConversionTrack(source)
    const reconstructed: CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: 'linear',keyframes: packed.keys.map((key,index) => ({ tick: key.t_us,value: key.value,interpolation: key.segment.kind === 'Spline' ? { kind: 'bezier',p1: key.out,p2: packed.keys[index+1]?.in_ ?? [2/3,2/3] } : key.segment.kind === 'Hold' ? 'hold' : 'linear' })) } }
    for (let time = 0;time <= 1_000_000;time += 10_000) expect(sampleAnimatableValue(reconstructed,time)).toBeCloseTo(sampleAnimatableValue(source,time),10)
    const unpacked = unpackConversionTrack(packed)
    for (let time = 0;time <= 1_000_000;time += 10_000) expect(sampleAnimatableValue(unpacked,time)).toBeCloseTo(sampleAnimatableValue(source,time),10)
    expect(JSON.stringify(source)).toBe(before)
  }
  const elastic = packConversionTrack({ mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'elastic',dir: 'in_out',amplitude: 1.5,period: 0.4 },extrapolation: { before: 'ping_pong',after: 'offset' },keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } })
  expect(elastic.keys[0]!.segment).toEqual({ kind: 'Elastic',dir: 'InOut',amplitude: 1.5,period: 0.4 })
  expect(elastic.extrapolate).toEqual({ before: 'PingPong',after: 'Offset' })
  expect(packPositionConversion({ mode: 'xy',x: { mode: 'constant',value: 2 },y: { mode: 'constant',value: 3 } },{ fpsNum: 30,fpsDen: 1,startFrame: 0,endFrame: 30,tolerancePx: 0.5,everyFrames: 1,xyMode: 'editable' }).tracks).toHaveLength(2)
})


it('unpacks detached native geometry and refuses missed tolerance or fractional grid keys', () => {
  const track = packConversionTrack({ mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } })
  const result = { nodes: [{ point: { x: 5,y: 7 },incoming: { x: -1,y: 2 },outgoing: { x: 3,y: 4 },cubic: true }],tracks: [track],sampleCount: 31,nodeCount: 1,maxErrorPx: 0.1,checkCount: 121,withinTolerance: true,limit: null }
  const before = JSON.stringify(result)
  const position = unpackPositionConversion(result,index => `node-${index}`)
  expect(position).toMatchObject({ mode: 'path',path: { nodes: [{ id: 'node-0',tangentMode: 'corner',segment: 'cubic' }] },progress: { track: { keyframes: [{ tangents: { continuity: 'broken',outgoing: { mode: 'free' } } },{}] } } })
  if (position.mode !== 'path') throw new Error('path required')
  ;(position.path.nodes[0]!.point as { x: number }).x = 99
  expect(JSON.stringify(result)).toBe(before)
  expect(() => unpackPositionConversion({ ...result,withinTolerance: false },String)).toThrow('tolerance')
  const fractional = { ...track,keys: track.keys.map((key,index) => ({ ...key,t_us: index ? 1_000_000.5 : 0 })) }
  expect(() => unpackConversionTrack(fractional)).toThrow('grid')
})

it('transports rational sample clocks and the original rounding grid without changing authored coordinates',()=>{
 const source:CompositionAnimatableValue={mode:'keyframes',track:{timeBase:1000,sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},interpolation:'linear',extrapolation:{before:'ping_pong',after:'offset'},keyframes:[{tick:250,value:0},{tick:1250,value:1}]}}
 const before=JSON.stringify(source),packed=packConversionTrack(source)
 expect(packed.sampleGrid).toEqual({timeBase:1000,rateNum:2,rateDen:1,offsetNum:250000})
 expect(packed.keys.map(key=>key.t_us)).toEqual([250000,1250000])
 const reconstructed=unpackConversionTrack(packed)
 const times=[0,1,249749,249750,250000,499749,500000,500001,1000000]
 for(const time of times)expect(sampleAnimatableValue(reconstructed,time)).toBe(sampleAnimatableValue(source,time))
 expect(sampleAnimatableValue(reconstructed,249750)).toBe(0.5)
 expect(JSON.stringify(source)).toBe(before)
 const request=packPositionConversion({mode:'path',path:{nodes:[{id:'a',point:{x:0,y:20},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'},{id:'b',point:{x:300,y:20},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]},progress:source},{fpsNum:30,fpsDen:1,startFrame:0,endFrame:30,tolerancePx:0.5,everyFrames:1,xyMode:'editable'})
 const xyPosition={mode:'xy' as const,x:{...source,track:{...source.track,keyframes:source.track.keyframes.map(key=>({...key,value:key.value*300}))}},y:{mode:'keyframes' as const,track:{timeBase:1000,sampleClock:{rateNum:3,rateDen:2,offsetNum:-250000},interpolation:'linear' as const,keyframes:[{tick:0,value:20},{tick:1000,value:80}]}}}
 const xyRequest=packPositionConversion(xyPosition,request.options)
 const oracle=process.env.NATIVE_POSITION_CONVERSION_CLOCK_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,request,xyRequest,xyPosition,position:{mode:'path',path:{nodes:request.nodes!.map((node,index)=>({id:String(index),point:node.point,inHandle:node.incoming,outHandle:node.outgoing,segment:node.cubic?'cubic':'line',tangentMode:'corner'}))},progress:source},samples:times.map(time=>({time,value:sampleAnimatableValue(source,time)}))}))
 const unclocked=packConversionTrack({mode:'keyframes',track:{timeBase:24,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:24,value:1}]}})
 expect(unclocked.sampleGrid).toEqual({timeBase:24,rateNum:1,rateDen:1,offsetNum:0})
})
