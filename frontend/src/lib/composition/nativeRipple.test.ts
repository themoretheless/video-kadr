import {afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
import {rippleNativeComposition} from './nativeRipple'
function project():api.NativeNestedProject{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'})
 doc=addTrack(doc,{id:'titles',kind:'text',name:'Titles',locked:false,hidden:false,clips:[]})
 for(const [id,start] of [['a',0],['b',2000000],['c',4000000]] as const)doc=addClip(doc,'titles',{id,kind:'text',timelineStartTicks:start,durationTicks:1000000,text:id,x:0,y:0,opacity:1,style:{fontSizePx:12,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc)
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},other:{content:structuredClone(content),instances:[]}}}
}
function mockMoves(){return vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
 const copy=structuredClone(project)
 for(const move of moves){const clip=copy.compositions[move.fromComposition]!.content.tracks.find(track=>track.id===move.fromTrackId)!.clips.find(clip=>clip.id===move.clipId)!,duration=nativeClipEnd(clip)-nativeClipStart(clip);Object.assign(clip,{timelineStartTick:move.startUs,timelineEndTick:move.startUs+duration})}
 return {project:copy} as Awaited<ReturnType<typeof api.moveNativeNestedClips>>
})}
afterEach(()=>vi.restoreAllMocks())
it('closes exact bounded gaps through native moves without changing other compositions',async()=>{
 const input=project(),before=JSON.stringify(input),move=mockMoves()
 const output=await rippleNativeComposition(input,'root',{trackId:'titles',start:1000000,end:2000000})
 expect(output.compositions.root!.content.tracks[0]!.clips.map(nativeClipStart)).toEqual([0,1000000,3000000])
 expect(move.mock.calls[0]![1].map(move=>move.clipId)).toEqual(['b','c'])
 expect(output.compositions.other).toBe(input.compositions.other);expect(JSON.stringify(input)).toBe(before)
 await expect(rippleNativeComposition(input,'root',{trackId:'titles',start:1100000,end:2000000})).rejects.toThrow('exact')
})
it('deletes selected intervals, merges holes and preserves rollback on locks and occupied gaps',async()=>{
 const input=project();mockMoves()
 const output=await rippleNativeComposition(input,'root',{clipIds:['b']})
 expect(output.compositions.root!.content.tracks[0]!.clips.map(clip=>[clip.id,nativeClipStart(clip)])).toEqual([['a',0],['c',3000000]])
 Object.assign(input.compositions.root!.content.tracks[0]!,{locked:true})
 await expect(rippleNativeComposition(input,'root',{clipIds:['b']})).rejects.toThrow('locked')
})
it('moves nested instances with the timeline and refuses links that would lose relative alignment',async()=>{
 const input=project();mockMoves()
 const clip=input.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual fixture required')
 input.compositions.root!.instances=[{id:'group',timing:{composition:'other',timelineStartUs:4000000,sourceInUs:0,sourceOutUs:1000000},transform:clip.transform,opacity:{mode:'constant',value:1},blendMode:'normal'}]
 const output=await rippleNativeComposition(input,'root',{trackId:'titles',start:1000000,end:2000000})
 expect(output.compositions.root!.instances[0]!.timing).toEqual({composition:'other',timelineStartUs:3000000,sourceInUs:0,sourceOutUs:1000000})
 Object.assign(input.compositions.root!.content,{metadata:{clipLinks:{linked:['a','c']}}})
 await expect(rippleNativeComposition(input,'root',{trackId:'titles',start:1000000,end:2000000})).rejects.toThrow('linked group')
 Object.assign(input.compositions.root!.content,{metadata:{}})
 input.compositions.root!.instances[0]!.timing.timelineStartUs=1500000
 await expect(rippleNativeComposition(input,'root',{trackId:'titles',start:1000000,end:2000000})).rejects.toThrow('inside')
})
