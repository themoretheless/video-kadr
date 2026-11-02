import {describe,it,expect} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/filmcraft-transition-insert.json'
import {insertTransitionWindow,transitionSourceSpan,transitionWindowProgress} from './transitionTimeline'
describe('original explicit transition clock',()=>{
 it('replays all eight original insertion windows and nullable endpoints',()=>{
  expect(fixture.commit).toBe('5231852443363f001c3f6b396dd9b1e6461ae2be');expect(fixture.cases).toHaveLength(8)
  const id=(value:number|null)=>value===null?null:`clip-${value}`
  for(const scenario of fixture.cases){
   const crossing=scenario.before.items.filter(item=>item.start<scenario.at&&scenario.at<item.start+item.duration)
   const right=scenario.after.items.find(item=>item.id>=2000&&item.id<100000)
   const splits=new Map(crossing.map(item=>[`clip-${item.id}`,`clip-${right!.id}`]))
   const actual=scenario.before.transitions.flatMap(item=>{
    const result=insertTransitionWindow({fromClipId:id(item.from),toClipId:id(item.to),timeline:{startTick:String(item.start),durationTick:String(item.duration)}},String(scenario.at),String(scenario.duration),splits)
    return result?[result]:[]
   })
   expect(actual,scenario.name).toEqual(scenario.after.transitions.map(item=>({fromClipId:id(item.from),toClipId:id(item.to),timeline:{startTick:String(item.start),durationTick:String(item.duration)}})))
  }
 })
 it('samples exact half-open intervals without microsecond rounding',()=>{
  const interval={startTick:'-1',durationTick:'3'}
  expect(transitionWindowProgress(interval,'-2')).toBeNull();expect(transitionWindowProgress(interval,'-1')).toBe(0)
  expect(transitionWindowProgress(interval,'0')).toBeCloseTo(1/3);expect(transitionWindowProgress(interval,'1')).toBeCloseTo(2/3)
  expect(transitionWindowProgress(interval,'2')).toBeNull();expect(transitionSourceSpan({startTick:-1,durationTick:2})).toEqual([-254016n,508032n])
 })
 it('rejects nonpositive durations and signed overflow',()=>{
  expect(()=>transitionSourceSpan({startTick:'0',durationTick:'0'})).toThrow()
  expect(()=>transitionSourceSpan({startTick:'9223372036854775807',durationTick:'1'})).toThrow()
  expect(()=>transitionSourceSpan({startTick:Number.MAX_SAFE_INTEGER,durationTick:'1'})).toThrow()
  expect(()=>insertTransitionWindow({fromClipId:'a',toClipId:'b',timeline:{startTick:'9223372036854775805',durationTick:'1'}},'0','2',new Map())).toThrow()
 })
})
