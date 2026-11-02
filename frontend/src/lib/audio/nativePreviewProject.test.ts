import {expect,it} from 'vitest'
import {createComposition} from '../composition/commands'
import type {Composition,CompositionSource} from '../composition/types'
import {buildNativeAudioPreviewProject,needsNativeAudioPreview} from './nativePreviewProject'
const keys=[{at:0,value:0.25},{at:1,value:1}]
const media:CompositionSource={id:'sound',kind:'audio',durationTicks:1000000,width:0,height:0,hasAudio:true}
function fixture():Composition{return {...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{sound:media},tracks:[{kind:'audio',id:'a',name:'Audio',locked:false,muted:false,clips:[{kind:'audio',id:'clip',sourceId:'sound',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,gain:0.9,concatGainKeys:keys}]}]}}
it('requests native preview for a source volume ride without DSP and carries it into wire audio',()=>{
 const c=fixture();expect(needsNativeAudioPreview(c)).toBe(true)
 const project=buildNativeAudioPreviewProject(c,{},1000000)
 expect(project.compositions.editor!.content.tracks[0]).toMatchObject({clips:[{concatGainKeys:keys,gain:{mode:'constant',value:0.9}}]})
 expect((project.compositions.editor!.content.tracks[0]!.clips[0] as {concatGainKeys:unknown}).concatGainKeys).not.toBe(keys)
 expect(needsNativeAudioPreview({...c,tracks:[]})).toBe(false)
 expect(needsNativeAudioPreview({...c,tracks:c.tracks.map(track=>({...track,muted:true}))})).toBe(false)
})
it('requests native mixing for an audible hidden video ride and respects source mute',()=>{
 const c=fixture(),video={...media,kind:'video' as const,width:16,height:16}
 const document:Composition={...c,sources:{sound:video},tracks:[{kind:'video',id:'v',name:'Video',hidden:true,locked:false,muted:false,transitions:[],clips:[{kind:'video',id:'clip',sourceId:'sound',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:true,audioGain:0.9,concatGainKeys:keys}]}]}
 expect(needsNativeAudioPreview(document)).toBe(true)
 expect(needsNativeAudioPreview({...document,sources:{sound:{...video,hasAudio:false}}})).toBe(false)
})

it('uses native preview for a selected stream even without source keys or DSP',()=>{
 const c=fixture();const selected:Composition={...c,tracks:c.tracks.map(track=>track.kind==='audio'?{...track,clips:track.clips.map(clip=>({...clip,concatGainKeys:undefined,audioStream:2}))}:track)}
 expect(needsNativeAudioPreview(selected)).toBe(true)
 expect(buildNativeAudioPreviewProject(selected,{},1000000).compositions.editor!.content.tracks[0]).toMatchObject({clips:[{audioStream:2}]})
})
