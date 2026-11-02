import {expect,it} from 'vitest'
import {stepComparisonReference} from './comparison'
it('steps project frames, preserves off-grid offset and clamps at zero',()=>{
 expect(stepComparisonReference(12345,1,{fpsMilli:25000})).toBe(52345)
 expect(stepComparisonReference(52345,-1,{fpsMilli:25000})).toBe(12345)
 expect(stepComparisonReference(12345,-1,{fpsMilli:25000})).toBe(0)
 expect(stepComparisonReference(0,1,{fpsMilli:29970,fpsRate:{numerator:30000,denominator:1001}})).toBe(33367)
})
it('rejects invalid and overflowing reference timing',()=>{
 expect(()=>stepComparisonReference(NaN,1,{fpsMilli:25000})).toThrow()
 expect(()=>stepComparisonReference(0,1,{fpsMilli:0})).toThrow()
 expect(()=>stepComparisonReference(Number.MAX_SAFE_INTEGER,1,{fpsMilli:25000})).toThrow()
})
