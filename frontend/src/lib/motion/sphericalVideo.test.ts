import {describe,it,expect} from 'vitest'
import {parseOrientationSidecar,parseLensCalibration,defaultLenses} from './sphericalVideo'
describe('spherical video sidecars',()=>{
 it('integrates constant calibrated angular velocity and preserves time',()=>{
  const result=parseOrientationSidecar({version:1,gyro:[{timeSeconds:0,radiansPerSecond:[0,Math.PI/2,0]},{timeSeconds:1,radiansPerSecond:[0,Math.PI/2,0]}]})
  expect(result[1].timeSeconds).toBe(1)
  expect(result[1].quaternion[0]).toBeCloseTo(Math.SQRT1_2,12)
  expect(result[1].quaternion[2]).toBeCloseTo(Math.SQRT1_2,12)
 })
 it('rejects ambiguous units, time order, invalid quaternions and excessive samples',()=>{
  for(const value of [{version:1,samples:[],gyro:[]},{version:1,samples:[{timeSeconds:0,quaternion:[0,0,0,0]}]},{version:1,gyro:[{timeSeconds:0,radiansPerSecond:[0,0,0]},{timeSeconds:0,radiansPerSecond:[0,0,0]}]},{version:1,gyro:[{timeSeconds:0,degreesPerSecond:[0,1,0]}]},{version:1,samples:Array(20001).fill({timeSeconds:0,quaternion:[1,0,0,0]})}])expect(()=>parseOrientationSidecar(value)).toThrow()
 })
 it('preserves a quaternion sequence and rejects invalid lens geometry',()=>{
  expect(parseOrientationSidecar({version:1,samples:[{timeSeconds:.5,quaternion:[-1,0,0,0]}]})).toEqual([{timeSeconds:.5,quaternion:[-1,0,0,0]}])
  expect(parseLensCalibration(defaultLenses)).toEqual(defaultLenses)
  expect(()=>parseLensCalibration([{...defaultLenses[0],radius:[0,.5]},defaultLenses[1]])).toThrow()
 })
})
