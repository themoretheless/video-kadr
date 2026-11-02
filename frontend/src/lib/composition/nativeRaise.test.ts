import {afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
import {raiseNativeLayers} from './nativeRaise'
function project():api.NativeNestedProject{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'})
 for(const id of ['first','second'])doc=addTrack(doc,{id,kind:'text',name:id,hidden:false,locked:false,clips:[]})
 for(const [id,track,start] of [['a','first',0],['b','second',2000000],['c','second',4000000]] as const)doc=addClip(doc,track,{id,kind:'text',timelineStartTicks:start,durationTicks:1000000,text:id,x:0,y:0,opacity:1,style:{fontSizePx:12,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc)
 return {schemaVersion:1,root:'root',compositions:{root:{content:{...content,metadata:{clipLinks:{group:['a','b','c']},captionTextTracks:['first']}},instances:[]},other:{content:structuredClone(content),instances:[]}}}
}
function mockMove(){return vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
 const copy=structuredClone(project),tracks=copy.compositions.root!.content.tracks
 for(const move of moves){const source=tracks.find(track=>track.id===move.fromTrackId)!,target=tracks.find(track=>track.id===move.toTrackId)!,clip=source.clips.find(clip=>clip.id===move.clipId)!,duration=nativeClipEnd(clip)-nativeClipStart(clip);Object.assign(source,{clips:source.clips.filter(clip=>clip.id!==move.clipId)});Object.assign(target,{clips:[...target.clips,{...clip,timelineStartTick:move.startUs,timelineEndTick:move.startUs+duration}]})}
 return {project:copy,changed:true,affectedCompositions:['root'],movedClipIds:moves.map(move=>move.clipId)}
})}
afterEach(()=>vi.restoreAllMocks())
it('raises exact layers to the highest new lane, removes empty sources and preserves links/caption designation',async()=>{
 const input=project(),before=JSON.stringify(input);mockMove()
 const result=await raiseNativeLayers(input,'root',['a','b'],'raised')
 expect(result.compositions.root!.content.tracks.map(track=>track.id)).toEqual(['raised','second'])
 expect(result.compositions.root!.content.tracks[0]!.clips.map(clip=>[clip.id,nativeClipStart(clip)])).toEqual([['a',0],['b',2000000]])
 expect(result.compositions.root!.content.metadata).toMatchObject({clipLinks:{group:['a','b','c']},captionTextTracks:['raised']})
 expect(result.compositions.other).toBe(input.compositions.other);expect(JSON.stringify(input)).toBe(before)
})
it('lands on an explicit selected anchor and rejects invalid anchors and reused lane IDs',async()=>{
 const input=project(),move=mockMove()
 const output=await raiseNativeLayers(input,'root',['a','b'],'raised',{clipId:'b',startTicks:3000000})
 expect(output.compositions.root!.content.tracks[0]!.clips.map(nativeClipStart)).toEqual([1000000,3000000])
 await expect(raiseNativeLayers(input,'root',['a'],'raised',{clipId:'b',startTicks:0})).rejects.toThrow('anchor')
 await expect(raiseNativeLayers(input,'root',['a'],'first')).rejects.toThrow('track ID')
 expect(move).toHaveBeenCalledTimes(1)
})

it('refuses an unexpected native no-op instead of committing an empty destination',async()=>{
 const input=project(),move=mockMove()
 move.mockResolvedValueOnce({project:input,changed:false,affectedCompositions:[],movedClipIds:[]})
 await expect(raiseNativeLayers(input,'root',['a'],'raised')).rejects.toThrow('did not move')
})
