import {expect,it} from 'vitest'
import {patchChromaSourceKey,setChromaSourceKey,removeChromaSourceKey,chromaRgb,chromaAlpha,chromaWithRgb,chromaWithAlpha,type ChromaEffect} from './concatChromaSourceKeys'
it('edits one channel while preserving other rides, held parameters and source ordering',()=>{
 const effect:ChromaEffect={kind:'concat_chroma_key',parameters:{spill:20},concatKeys:{similarity:[{at:1,value:60,ease:'out'},{at:0,value:1}],softness:[{at:0,value:5,ease:'in'}]}}
 const before=JSON.stringify(effect);const changed=patchChromaSourceKey(effect,'similarity',1,{at:0.25,value:10})
 expect(changed.parameters).toBe(effect.parameters);expect(changed.concatKeys?.softness).toBe(effect.concatKeys?.softness)
 expect(changed.concatKeys?.similarity).toEqual([{at:1,value:60,ease:'out'},{at:0.25,value:10}]);expect(JSON.stringify(effect)).toBe(before)
 const inserted=setChromaSourceKey(changed,'softness',{at:0.001,value:8,ease:'inOut'})
 expect(inserted.concatKeys?.softness).toEqual([{at:0.001,value:8,ease:'inOut'}])
 expect(removeChromaSourceKey(inserted,'softness',0).concatKeys?.similarity).toEqual(changed.concatKeys?.similarity)
 expect(()=>patchChromaSourceKey(effect,'similarity',0,{value:61})).toThrow()
})
it('authors all unsigned RGBA bytes without signed32 coercion or losing alpha',()=>{
 expect(chromaWithRgb(0x00b140ff,'#ffffff')).toBe(4294967295)
 expect(chromaWithRgb(4294967295,'#0000ff')).toBe(65535)
 const red=chromaWithRgb(chromaWithAlpha(65535,127),'#ff0000')
 expect(red).toBe(4278190207);expect(chromaRgb(red)).toBe('#ff0000');expect(chromaAlpha(red)).toBe(127)
 expect(chromaWithAlpha(red,255)).toBe(4278190335)
 expect(()=>chromaWithAlpha(red,256)).toThrow()
})
it('bounds aggregate source keys while allowing a nearest-key replacement at the limit',()=>{
 const effect:ChromaEffect={kind:'concat_chroma_key',parameters:{},concatKeys:{color:Array.from({length:4096},()=>({at:0,value:0}))}}
 expect(()=>setChromaSourceKey(effect,'spill',{at:1,value:50})).toThrow()
 expect(setChromaSourceKey(effect,'color',{at:0,value:4294967295}).concatKeys?.color).toHaveLength(4096)
})
