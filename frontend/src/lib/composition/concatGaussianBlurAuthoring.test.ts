import {expect,it} from 'vitest'
import {keyGaussianBlurRadius,sampleGaussianBlurRadius,setGaussianBlurRadius,type GaussianBlurEffect} from './concatGaussianBlurAuthoring'
it('authors a riding radius on its existing time base without changing base or other keys',()=>{
 const original:GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{radius:30},animation:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:0,value:1,interpolation:'hold'},{tick:1000,value:10}]}}}
 const before=JSON.stringify(original)
 const changed=setGaussianBlurRadius(original,500000,5.9)
 expect(changed.parameters.radius).toBe(30)
 expect(changed.animation).toMatchObject({mode:'keyframes',track:{timeBase:1000,keyframes:[{tick:0,value:1,interpolation:'hold'},{tick:500,value:5.9,interpolation:'hold'},{tick:1000,value:10}]}})
 expect(JSON.stringify(original)).toBe(before)
 expect(sampleGaussianBlurRadius(changed,500000)).toBe(5.9)
 const replaced=keyGaussianBlurRadius(changed,500000,7)
 if(replaced.animation?.mode!=='keyframes')throw new Error('keyframes')
 expect(replaced.animation.track.keyframes).toHaveLength(3)
 expect(replaced.animation.track.keyframes[1]).toEqual({tick:500,value:7,interpolation:'hold'})
})
it('samples existing animation when adding a key, and retains constant authoring',()=>{
 const effect:GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{radius:30},animation:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:1},{tick:1000000,value:11}]}}}
 const changed=keyGaussianBlurRadius(effect,500000)
 if(changed.animation?.mode!=='keyframes')throw new Error('keyframes')
 expect(changed.animation.track.keyframes[1]?.value).toBe(6)
 expect(setGaussianBlurRadius({kind:'concat_gaussian_blur',parameters:{},animation:{mode:'constant',value:2}},0,5.9).animation).toEqual({mode:'constant',value:5.9})
 expect(()=>keyGaussianBlurRadius(effect,-1)).toThrow()
 expect(()=>setGaussianBlurRadius(effect,0,51)).toThrow()
})
it('preserves source Concat keys rather than silently authoring a competing native curve',()=>{
 const effect:GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{radius:10},concatKeys:[{at:0,value:1},{at:1,value:10,ease:'out'}]}
 const before=JSON.stringify(effect)
 expect(()=>setGaussianBlurRadius(effect,500000,5)).toThrow()
 expect(()=>keyGaussianBlurRadius(effect,500000,5)).toThrow()
 expect(JSON.stringify(effect)).toBe(before)
})
