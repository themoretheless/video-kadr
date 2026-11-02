import {expect,it} from 'vitest'
import {validateVideoFrameTimes,videoRegionOrdinal} from './video-region'
it('holds a VFR source plane until the next presentation timestamp',()=>{
 const clock={frames:3,frameRate:{numerator:10,denominator:1},frameTimesUs:[0,100000,400000,500000]}
 for(const [time,ordinal] of [[0,0],[99999,0],[100000,1],[250000,1],[399999,1],[400000,2],[499999,2]])expect(videoRegionOrdinal(clock,time!)).toBe(ordinal)
 expect(()=>videoRegionOrdinal(clock,500000)).toThrow();expect(()=>videoRegionOrdinal(clock,-1)).toThrow()
 for(const times of [[1,100000,400000,500000],[0,100000,100000,500000],[0,100000,500000],[0,100000,400000,Number.MAX_SAFE_INTEGER+1],null])expect(()=>validateVideoFrameTimes(times,3)).toThrow()
 expect(videoRegionOrdinal({frames:3,frameRate:{numerator:30000,denominator:1001}},33367)).toBe(1)
})
