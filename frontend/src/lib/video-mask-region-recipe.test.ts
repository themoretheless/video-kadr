import {expect,it,vi} from 'vitest'
import {videoRegionRecipe} from './components/composition/test-fixtures/videoRegionRecipe'
import {canonicalVideoRecipe,copyVideoCorrectionRange,copyVideoRegionCorrectionRange,validateVideoAttachment,validateVideoRecipe} from './video-mask-recipe'
it('retains plane provenance in canonical recipes and admits only inventory-bound annotations',()=>{
 const value=videoRegionRecipe();validateVideoRecipe(value,4)
 validateVideoAttachment({...value.projectOrigin!.snapshot,corrections:value.corrections})
 expect(JSON.parse(canonicalVideoRecipe(value.corrections))[0].correction.strokes[0].region).toEqual(value.corrections[0]!.correction.strokes[0]!.region)
 const changed=structuredClone(value);Object.assign(changed.corrections[0]!.correction.strokes[0]!.region!,{maskHash:'4'.repeat(64)})
 expect(canonicalVideoRecipe(changed.corrections)).not.toBe(canonicalVideoRecipe(value.corrections))
 expect(()=>validateVideoRecipe({...value,projectOrigin:undefined},4)).toThrow('привязка')
 for(const field of ['sourceId','maskSourceId','sourceHash','maskFileHash','encoderHash','decoderHash','runtimeHash','promptHash'] as const){const bad=structuredClone(value);Object.assign(bad.corrections[0]!.correction.strokes[0]!.region!,{[field]:field.endsWith('Id')?'other':'5'.repeat(64)});expect(()=>validateVideoRecipe(bad,4),field).toThrow('привязка')}
 const plain=structuredClone(value);plain.corrections[0]!.correction.strokes[0]!.tool='brush';expect(()=>validateVideoRecipe(plain,4)).toThrow('привязка')
})
it('refuses copying an ordinal plane hash over another frame without losing the original recipe',()=>{
 const value=videoRegionRecipe(),before=JSON.stringify(value)
 expect(()=>copyVideoCorrectionRange(value,4,1,1,3)).toThrow('каждому кадру')
 expect(JSON.stringify(value)).toBe(before)
 expect(canonicalVideoRecipe(copyVideoCorrectionRange(value,4,1,1,2))).toBe(canonicalVideoRecipe(value.corrections))
})

it('binds every ordinal independently, shares repeated region reads and preserves stroke ordering and input',async()=>{
 const value=videoRegionRecipe(),source=value.corrections[0]!.correction,smart=source.strokes[0]!
 source.strokes.push({...structuredClone(smart),tool:'smartEraser'},{tool:'brush',size:.1,points:[[.2,.3]],at:null})
 const before=JSON.stringify(value),resolve=vi.fn(async(_id:string,index:number)=>String(index+4).repeat(64)),progress=vi.fn()
 const result=await copyVideoRegionCorrectionRange(value,4,1,0,4,value.projectOrigin!.snapshot.trackedRegions!,resolve,new AbortController().signal,progress)
 expect(resolve).toHaveBeenCalledTimes(4);expect(result.map(e=>e.frame)).toEqual([0,1,2,3])
 for(const edit of result){expect(edit.correction.strokes.map(s=>s.tool)).toEqual(['smartBrush','smartEraser','brush']);expect(edit.correction.strokes.slice(0,2).map(s=>s.region!.maskHash)).toEqual([String(edit.frame+4).repeat(64),String(edit.frame+4).repeat(64)])}
 expect(progress).toHaveBeenLastCalledWith(4,4);expect(JSON.stringify(value)).toBe(before);validateVideoRecipe({...value,corrections:result},4)
})
it('aborts after a resolved plane without returning partial edits or reading later frames',async()=>{
 const value=videoRegionRecipe(),before=JSON.stringify(value),controller=new AbortController(),resolve=vi.fn(async()=>{controller.abort();return '4'.repeat(64)})
 await expect(copyVideoRegionCorrectionRange(value,4,1,0,4,value.projectOrigin!.snapshot.trackedRegions!,resolve,controller.signal)).rejects.toMatchObject({name:'AbortError'})
 expect(resolve).toHaveBeenCalledTimes(1);expect(JSON.stringify(value)).toBe(before)
})
it.each(['hash','transport'])('preserves the input after a %s failure midway through preparation',async(kind)=>{
 const value=videoRegionRecipe(),before=JSON.stringify(value),resolve=vi.fn(async(_id:string,index:number)=>{if(index===2){if(kind==='transport')throw new Error('offline');return 'wrong'}return '4'.repeat(64)})
 await expect(copyVideoRegionCorrectionRange(value,4,1,0,4,value.projectOrigin!.snapshot.trackedRegions!,resolve,new AbortController().signal)).rejects.toThrow()
 expect(JSON.stringify(value)).toBe(before);expect(resolve).toHaveBeenCalledTimes(3)
})
it('refuses an oversized expanded recipe before reading any region plane',async()=>{
 const value=videoRegionRecipe(),region={...value.projectOrigin!.snapshot.trackedRegions![0]!,frames:2000,trackedFrames:2000},resolve=vi.fn(async()=> '4'.repeat(64))
 await expect(copyVideoRegionCorrectionRange(value,2000,1,0,1025,[region],resolve,new AbortController().signal)).rejects.toThrow('предел')
 expect(resolve).not.toHaveBeenCalled()
})
