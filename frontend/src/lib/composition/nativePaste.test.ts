import {afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {pasteNativeLayers} from './nativePaste'
function project():api.NativeNestedProject{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'})
 doc=addTrack(doc,{id:'titles',kind:'text',name:'Titles',locked:false,hidden:false,clips:[]})
 for(const [id,start] of [['a',0],['b',2000000],['c',4000000]] as const)doc=addClip(doc,'titles',{id,kind:'text',timelineStartTicks:start,durationTicks:1000000,text:id,x:0,y:0,opacity:1,style:{fontSizePx:12,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc)
 return {schemaVersion:1,root:'root',compositions:{root:{content:{...content,metadata:{clipLinks:{group:['a','b','c']}}},instances:[]},other:{content:structuredClone(content),instances:[]}}}
}
afterEach(()=>vi.restoreAllMocks())
it('stages exact requested copy IDs and selected link subsets without changing the source',async()=>{
 const input=project(),before=JSON.stringify(input)
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(staged,moves)=>{
  expect(moves.map(move=>[move.clipId,move.startUs])).toEqual([['b-copy',3000000],['a-copy',1000000]])
  const scratch=staged.compositions[moves[0]!.fromComposition]!
  expect(scratch.content.metadata?.clipLinks).toEqual({'copied-link':['a-copy','b-copy']})
  expect(staged.compositions.root!.content.metadata?.clipLinks).toEqual({group:['a','b','c']})
  expect(scratch.content.tracks[0]!.clips.map(clip=>[clip.id,'text' in clip&&clip.text])).toEqual([['a-copy','a'],['b-copy','b']])
  return {project:staged,changed:true,affectedCompositions:['root'],movedClipIds:moves.map(move=>move.clipId)}
 })
 const result=await pasteNativeLayers(input,'root',[{clipId:'b',id:'b-copy'},{clipId:'a',id:'a-copy'}],{startTicks:3000000,linkIds:['copied-link']})
 expect(move).toHaveBeenCalledTimes(1);expect(result.compositions.other).toBe(input.compositions.other);expect(JSON.stringify(input)).toBe(before)
})
it('refuses invalid subjects, missing/locked targets and mismatched link IDs before native admission',async()=>{
 const input=project(),move=vi.spyOn(api,'moveNativeNestedClips')
 for(const subjects of [[{clipId:'a',id:'a'}],[{clipId:'a',id:'x'},{clipId:'a',id:'y'}],[{clipId:'missing',id:'x'}]])await expect(pasteNativeLayers(input,'root',subjects)).rejects.toThrow()
 await expect(pasteNativeLayers(input,'root',[{clipId:'a',id:'x'},{clipId:'b',id:'y'}],{linkIds:[]})).rejects.toThrow('link IDs')
 await expect(pasteNativeLayers(input,'root',[{clipId:'a',id:'x'}],{trackId:'missing'})).rejects.toThrow('target')
 expect(move).not.toHaveBeenCalled()
})
