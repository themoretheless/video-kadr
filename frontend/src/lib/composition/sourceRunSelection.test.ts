import {expect,it} from 'vitest'
import {sourceRunDelta} from './sourceRunSelection'
it('clamps a whole indexed selection at normalized walls while preserving duplicates',()=>{
 const keys=[{at:.1,value:1},{at:.1,value:2},{at:.8,value:3}]
 const moved=sourceRunDelta(keys,[0,1,2],.5,0);expect(moved.map(edit=>edit.index)).toEqual([0,1,2]);expect(moved[0]!.change.at).toBeCloseTo(.3);expect(moved[1]!.change.at).toBeCloseTo(.3);expect(moved[2]!.change.at).toBe(1)
 expect(sourceRunDelta(keys,[0,1],-.5,0)).toEqual([{index:0,change:{at:0}},{index:1,change:{at:0}}]);expect(keys[0]).toEqual({at:.1,value:1})
})
it('omits unchanged fields and rejects invalid selection or overflowing values before authoring',()=>{
 expect(sourceRunDelta([{at:.25,value:1}],[0],0,0)).toEqual([]);expect(sourceRunDelta([{at:.25,value:1}],[0],0,.5)).toEqual([{index:0,change:{value:1.5}}])
 expect(()=>sourceRunDelta([{at:0,value:1}],[0,0],0,0)).toThrow('выделение');expect(()=>sourceRunDelta([{at:0,value:1}],[1],0,0)).toThrow('выделение');expect(()=>sourceRunDelta([{at:0,value:Number.MAX_VALUE}],[0],0,Number.MAX_VALUE)).toThrow('диапазона')
})
