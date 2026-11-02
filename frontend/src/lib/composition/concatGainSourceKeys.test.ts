import {expect,it} from 'vitest'
import {cloneGainSourceKeys,patchGainSourceKey,upsertGainSourceKey} from './concatGainSourceKeys'
it('edits the closest epsilon key while preserving incoming ease and immutable inputs',()=>{
 const keys=[{at:0.5,value:1,ease:[0.42,-0.5,0.58,1] as const},{at:0.501,value:2}]
 const out=upsertGainSourceKey(keys,0.5012,3);expect(out).toHaveLength(2);expect(out[1]).toMatchObject({at:0.5012,value:3});expect(keys[1]!.value).toBe(2)
 const patched=patchGainSourceKey(keys,0,{value:4});expect(patched[0]).toMatchObject({ease:keys[0]!.ease,value:4,derived:false});expect(patched[0]!.ease).not.toBe(keys[0]!.ease)
})
it('retains derived boundaries while editing position/ease and checks authored values',()=>{
 const keys=[{at:0,value:-2,derived:true}]
 expect(patchGainSourceKey(keys,0,{at:0.2})[0]).toMatchObject({value:-2,derived:true})
 expect(()=>patchGainSourceKey(keys,0,{value:-1})).toThrow();expect(()=>patchGainSourceKey(keys,0,{at:2})).toThrow()
 expect(()=>patchGainSourceKey(keys,0,{ease:[-1,0,1,1]})).toThrow();expect(()=>patchGainSourceKey(keys,2,{})).toThrow()
})
it('allows replacing a key at the budget but refuses growing past it',()=>{
 const keys=Array.from({length:4096},(_,i)=>({at:i/4096,value:1}))
 expect(upsertGainSourceKey(keys,0,2)).toHaveLength(4096)
 expect(()=>cloneGainSourceKeys([...keys,{at:1,value:1}])).toThrow('4096')
})
