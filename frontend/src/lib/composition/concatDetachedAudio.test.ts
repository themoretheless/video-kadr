import {readFileSync,writeFileSync} from 'node:fs'
import {expect,it} from 'vitest'
import {createComposition} from './commands'
import type {Composition} from './types'
import type {NativeNestedProject} from '$lib/api'
import {buildCompositionWireDocument} from './payload'
import {reattachConcatAudio,reattachNativeConcatAudio} from './concatDetachedAudio'
import {reattachAudio} from './reattachAudio'
import {expandNativeClipLinks} from './nativeClipLinks'
function fixture():Composition{return {...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:true}},tracks:[{kind:'video',id:'v',name:'Video',hidden:false,muted:false,locked:false,transitions:[],clips:[{kind:'video',id:'picture',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:0.7}]},{kind:'audio',id:'a',name:'Audio',muted:false,locked:false,clips:[{kind:'audio',id:'late',sourceId:'source',timelineStartTicks:3000000,sourceInTicks:0,sourceOutTicks:1000000,gain:0.2,detachedFrom:'picture',concatDetachedOrder:2,dspEffects:[{id:'concat.hall'}]},{kind:'audio',id:'first',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,gain:0.4,detachedFrom:'picture',concatDetachedOrder:1,dspEffects:[{id:'concat.echo',params:{delay:0.25}}]}]}]}}
function nested(document=fixture()):NativeNestedProject{return {schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(document),instances:[]}}}}
it('restores the first source filter chain without transferring edited sound gain/windows',()=>{
 const input=fixture(),before=JSON.stringify(input),out=reattachAudio(input,'late')
 expect(out.videoId).toBe('picture');expect(out.document.tracks[0]).toMatchObject({clips:[{audioGain:0.7,sourceAudioEnabled:true,independentAudio:true,dspEffects:[{id:'concat.echo',params:{delay:0.25}}]}]})
 expect(out.document.tracks[1]!.clips).toHaveLength(0);expect(JSON.stringify(input)).toBe(before)
})
it('keeps provenance loose, removes all fragments and preserves unrelated native metadata',()=>{
 const input=nested();expect(expandNativeClipLinks(input,'root',['first'])).toEqual(['first'])
 const out=reattachNativeConcatAudio(input,'root','picture');expect(out.compositions.root!.content.tracks[1]!.clips).toHaveLength(0)
 expect(out.compositions.root!.content.tracks[0]).toMatchObject({clips:[{audioGain:{value:0.7},dspEffects:[{id:'concat.echo'}],sourceAudioEnabled:true}]})
 expect(input.compositions.root!.content.tracks[1]!.clips).toHaveLength(2)
})
it('refuses missing origins, locked clips/tracks and explicit linked selections',()=>{
 const input=fixture();expect(()=>reattachConcatAudio(input,'missing')).toThrow()
 expect(()=>reattachConcatAudio({...input,tracks:input.tracks.map(track=>({...track,locked:true}))},'picture')).toThrow('заблокированы')
 expect(()=>reattachConcatAudio({...input,links:[{id:'manual',memberIds:['picture','first']}]},'picture')).toThrow('явную связь')
 const project=nested();const node=project.compositions.root!;node.content={...node.content,tracks:node.content.tracks.map(track=>({...track,locked:true}))};expect(()=>reattachNativeConcatAudio(project,'root','first')).toThrow('заблокированы')
})
it('reattaches the actual imported source document in original fragment order',()=>{
 const path=process.env.CONCAT_DETACHED_PROJECT_INPUT;if(!path)return
 const input=JSON.parse(readFileSync(path,'utf8')) as NativeNestedProject,root=input.root
 expect(expandNativeClipLinks(input,root,['sound-b'])).toEqual(['sound-b'])
 const output=reattachNativeConcatAudio(input,root,'sound-a')
 expect(output.compositions[root]!.content.tracks[0]).toMatchObject({clips:[{id:'clip',sourceAudioEnabled:true,dspEffects:[{id:'concat.echo'}]}]})
 expect(output.compositions[root]!.content.tracks.filter(track=>track.kind==='audio').every(track=>track.clips.length===0)).toBe(true)
 if(process.env.CONCAT_DETACHED_REJOINED_PROJECT_OUTPUT)writeFileSync(process.env.CONCAT_DETACHED_REJOINED_PROJECT_OUTPUT,JSON.stringify(output))
})

it('preserves the video stream choice on source-provenance reattachment',()=>{
 const input=fixture();const changed:Composition={...input,tracks:input.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>({...clip,audioStream:2}))}:track.kind==='audio'?{...track,clips:track.clips.map(clip=>({...clip,audioStream:1}))}:track)}
 expect(reattachConcatAudio(changed,'first').document.tracks[0]).toMatchObject({clips:[{audioStream:2}]})
 expect(reattachNativeConcatAudio(nested(changed),'root','first').compositions.root!.content.tracks[0]).toMatchObject({clips:[{audioStream:2}]})
})
