import {expect,it} from 'vitest'
import {parseDriftCorrection} from './gyroOptions'
it('keeps observed stationary windows and headings explicit',()=>{
 expect(parseDriftCorrection({stationaryIntervals:[{startSeconds:0,endSeconds:1}],headingAnchors:[{timeSeconds:0,headingDegrees:179},{timeSeconds:2,headingDegrees:-179}]},2)).toMatchObject({stationaryIntervals:[{startSeconds:0,endSeconds:1}]})
 for(const value of [{biasRadiansPerSecond:[0,0,.3]},{stationaryIntervals:[{startSeconds:0,endSeconds:.1}]},{headingAnchors:[{timeSeconds:0,headingDegrees:0}]},{biasRadiansPerSecond:[0,0,0],stationaryIntervals:[{startSeconds:0,endSeconds:1}]},{unknown:true}])expect(()=>parseDriftCorrection(value,2)).toThrow()
})
