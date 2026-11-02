import {expect,it} from 'vitest'
import {patchSourceRadiusKey,setSourceRadiusKey,removeSourceRadiusKey,sourceEasePoints,sourceEaseName} from './concatRadiusSourceKeys'
import type {GaussianBlurEffect} from './concatGaussianBlurAuthoring'
it('edits a source key without rewriting other keys, base settings or source easing',()=>{
 const effect:GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{radius:30},concatKeys:[{at:1,value:10,ease:'out'},{at:0,value:1,ease:[0.42,-0.5,0.58,1]}]}
 const before=JSON.stringify(effect)
 const changed=patchSourceRadiusKey(effect,1,{at:0.25,value:5.9})
 expect(changed.parameters).toBe(effect.parameters)
 expect(changed.concatKeys?.[0]).toBe(effect.concatKeys?.[0])
 expect(changed.concatKeys?.[1]).toEqual({at:0.25,value:5.9,ease:[0.42,-0.5,0.58,1]})
 expect(changed.animation).toBeUndefined();expect(JSON.stringify(effect)).toBe(before)
 expect(()=>patchSourceRadiusKey(effect,0,{value:51})).toThrow()
})
it('replaces only the nearest key inside the original epsilon and preserves stable duplicate order',()=>{
 const effect:GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{},concatKeys:[{at:0.5,value:2},{at:0.501,value:3},{at:0.501,value:4}]}
 const changed=setSourceRadiusKey(effect,{at:0.5011,value:8,ease:'in'})
 expect(changed.concatKeys).toEqual([{at:0.5,value:2},{at:0.501,value:4},{at:0.5011,value:8,ease:'in'}])
 expect(removeSourceRadiusKey(changed,1).concatKeys).toHaveLength(2)
 expect(removeSourceRadiusKey({kind:'concat_gaussian_blur',parameters:{},concatKeys:[{at:0,value:1}]},0).concatKeys).toBeUndefined()
 expect(()=>setSourceRadiusKey({...effect,animation:{mode:'constant',value:2}},{at:0,value:1})).toThrow()
})
it('reads presets and arbitrary finite overshoot without evaluating or flattening the curve',()=>{
 expect(sourceEasePoints({at:0,value:1,ease:'out'})).toEqual([0,0,0.58,1])
 expect(sourceEasePoints({at:0,value:1,ease:[0.42,-2,0.58,3]})).toEqual([0.42,-2,0.58,3])
 expect(sourceEaseName({at:0,value:1,ease:[0.42,-2,0.58,3]})).toBe('custom')
 expect(sourceEaseName({at:0,value:1,ease:[0.421,0,0.579,1]})).toBe('inOut')
})
