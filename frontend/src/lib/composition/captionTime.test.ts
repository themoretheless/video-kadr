import originalMinimum from '../../../../backend/tests/fixtures/filmcraft-caption-frame-minimum.json'
import {expect,it} from 'vitest'
import {captionDisplaySeconds,compareCaptionTimes,exactCaptionTicks,exactCaptionDeltaTicks,captionFrameDelta} from './captionTime'
it('compares both wire representations exactly, including submicrosecond and beyond-2^53 ticks',()=>{
 expect(compareCaptionTimes(1,'254016')).toBe(0)
 expect(compareCaptionTimes(1,'254015')).toBe(1)
 expect(compareCaptionTimes(1,'254017')).toBe(-1)
 expect(compareCaptionTimes('9007199254740993','9007199254740992')).toBe(1)
 expect(exactCaptionTicks('-9007199254740993')).toBe(-9007199254740993n)
 expect(compareCaptionTimes('137729592000',542208)).toBe(1)
 expect(captionDisplaySeconds('137729592000')).toBeCloseTo(13*1001/24000,12)
})
it('refuses lossy JSON numbers and malformed or overflowing source timestamps',()=>{
 for(const value of [0.1,-1,9007199254740992,'0.1','NaN','9223372036854775808','-9223372036854775809'])expect(()=>exactCaptionTicks(value)).toThrow()
})

it('keeps signed edit distances distinct from nonnegative numeric timestamps',()=>{
 expect(()=>exactCaptionTicks(-1)).toThrow()
 expect(exactCaptionDeltaTicks(-1)).toBe(-254016n)
 expect(exactCaptionDeltaTicks('-1')).toBe(-1n)
 expect(exactCaptionDeltaTicks('-137729592000')).toBe(-137729592000n)
 expect(()=>exactCaptionDeltaTicks(-9007199254740992)).toThrow()
})

it('matches independently executed original source frame boundaries without host-us rounding',()=>{
 for(const scenario of originalMinimum.cases){
  const rate={numerator:scenario.rate.num,denominator:scenario.rate.den}
  expect(captionFrameDelta(1,rate)).toBe(scenario.minimum)
  expect(captionFrameDelta(13,rate)).toBe(scenario.floor)
  expect(captionFrameDelta(-1,rate)).toBe((-BigInt(scenario.minimum)).toString())
 }
 expect(captionFrameDelta(-1,{numerator:11,denominator:1})).toBe('-23092363637')
 expect(()=>captionFrameDelta(0.5,{numerator:24,denominator:1})).toThrow()
 expect(()=>captionFrameDelta(1,{numerator:0,denominator:1})).toThrow()
 expect(()=>captionFrameDelta(Number.MAX_SAFE_INTEGER,{numerator:1,denominator:1})).toThrow()
})
