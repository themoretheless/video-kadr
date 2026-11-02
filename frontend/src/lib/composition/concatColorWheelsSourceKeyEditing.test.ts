import {expect,it} from 'vitest'
import {patchColorWheelsSourceKey,setColorWheelsSourceKey,removeColorWheelsSourceKey,type ColorWheelsEffect} from './concatColorWheelsSourceKeyEditing'
it('preserves the other channel, duplicates and easing and enforces authored limits',()=>{
 const effect:ColorWheelsEffect={kind:'concat_color_wheels',parameters:{lift:[0.1,0.2,0.3]},concatKeys:{'lift.x':[{at:0.5,value:0.1},{at:0.5,value:0.2,ease:[0.42,-2,0.58,3]}],'gain.m':[{at:0,value:0.2}]}}
 const patched=patchColorWheelsSourceKey(effect,'lift.x',1,{value:0.3});expect(patched.parameters).toBe(effect.parameters);expect(patched.concatKeys?.['gain.m']).toBe(effect.concatKeys?.['gain.m']);expect(patched.concatKeys?.['lift.x']?.[1]?.ease).toEqual([0.42,-2,0.58,3]);expect(effect.concatKeys?.['lift.x']?.[1]?.value).toBe(0.2)
 expect(setColorWheelsSourceKey(effect,'lift.x',{at:0.501,value:0.4}).concatKeys?.['lift.x']).toHaveLength(2)
 expect(removeColorWheelsSourceKey(effect,'gain.m',0).concatKeys?.['gain.m']).toBeUndefined()
 expect(()=>setColorWheelsSourceKey(effect,'lift.x',{at:0,value:-1.01})).toThrow();expect(()=>setColorWheelsSourceKey(effect,'gain.m',{at:0,value:101})).toThrow()
 expect(()=>setColorWheelsSourceKey({...effect,animation:{'gain.m':{mode:'constant',value:0}}},'lift.x',{at:0,value:0.1})).toThrow()
 const full:ColorWheelsEffect={...effect,concatKeys:{'gain.m':Array.from({length:4096},()=>({at:0,value:0}))}};expect(()=>setColorWheelsSourceKey(full,'lift.x',{at:1,value:0.1})).toThrow()
})
