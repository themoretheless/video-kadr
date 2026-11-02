import {expect,it} from 'vitest'
import {captureSourceRunClipboard,pasteSourceRunClipboard,parseSourceRunClipboard,serializeSourceRunClipboard} from './sourceRunClipboard'
it('snapshots relative normalized positions, original duplicate order, easing spelling and derived metadata without aliases',()=>{
 const keys=[{at:.7,value:2,ease:'out' as const},{at:.2,value:-1,derived:true,ease:[.42,-.5,.58,1] as [number,number,number,number]},{at:.2,value:3,ease:'linear' as const}],copy=captureSourceRunClipboard(keys,[2,1])
 expect(copy.keys).toEqual([{at:0,value:-1,derived:true,ease:[.42,-.5,.58,1]},{at:0,value:3,ease:'linear'}]);expect(copy.keys[0]!.ease).not.toBe(keys[1]!.ease);expect(parseSourceRunClipboard(serializeSourceRunClipboard(copy))).toEqual(copy)
})
it('pastes duplicate source keys without repeatedly overwriting the same target',()=>{
 const copy=captureSourceRunClipboard([{at:.1,value:1},{at:.1,value:2}],[0,1]),source=[{at:.5,value:0,ease:'in' as const},{at:.9,value:3}],out=pasteSourceRunClipboard(source,copy,.5)
 expect(out).toEqual([{at:.5,value:1},{at:.5,value:2},{at:.9,value:3}]);expect(source[0]).toEqual({at:.5,value:0,ease:'in'})
})
it('applies nearest-key epsilon once per original target and refuses overflow without clipping the source fragment',()=>{
 const copy=captureSourceRunClipboard([{at:0,value:1},{at:.5,value:2}],[0,1]),target=[{at:.5005,value:9}]
 expect(pasteSourceRunClipboard(target,copy,.5)).toEqual([{at:.5,value:1},{at:1,value:2}]);expect(()=>pasteSourceRunClipboard(target,copy,.6)).toThrow('конец');expect(()=>pasteSourceRunClipboard(target,copy,NaN)).toThrow('Позиция');expect(target).toEqual([{at:.5005,value:9}])
})
it.each([`{"format":"wrong","version":1,"keys":[{"at":0,"value":1}]}`,`{"format":"video-kadr/concat-source-keys","version":1,"keys":[{"at":0,"value":null}]}`,`{"format":"video-kadr/concat-source-keys","version":1,"keys":[{"at":0,"value":1,"ease":[-1,0,1,1]}]}`,`{"format":"video-kadr/concat-source-keys","version":1,"keys":[{"at":0,"value":1,"derived":"true"}]}`])('refuses malformed clipboard input %s',text=>{expect(()=>parseSourceRunClipboard(text)).toThrow()})
