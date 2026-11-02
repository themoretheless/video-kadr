import { beforeEach, expect, it } from 'vitest'
import { restyleCaptions, restyleNestedCaptions } from './restyleCaptions'
import { buildCompositionWireDocument } from './payload'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests } from '../state/composition/core.svelte'
import { importSrtToComposition } from '../state/composition/timeline.svelte'
import type { NativeNestedProject } from '../api'
import type { TextTrack } from './types'
beforeEach(()=>{setCompositionStorageForTests(null);resetCompositionForTests();importSrtToComposition('1\n00:00:00,000 --> 00:00:03,000\nOne\n\n2\n00:00:01,000 --> 00:00:02,000\nTwo\n\n3\n00:00:04,000 --> 00:00:05,000\nThree')})
it('restyles explicit unique cues across lanes and leaves other text and clocks intact',()=>{
 const document=compositionState.document, before=JSON.stringify(document)
 const first=(document.tracks[0] as TextTrack).clips[0]!
 const result=restyleCaptions(document,{font_size_px:99,font_family:null},[first.id,first.id])
 expect(result).toMatchObject({captions:3,restyled:1})
 expect((result.document.tracks[0] as TextTrack).clips[0]).toEqual({...first,style:{...first.style,fontSizePx:99}})
 expect(result.document.tracks[1]).toEqual(document.tracks[1])
 expect(JSON.stringify(document)).toBe(before)
})
it('removes color animation for static color and preserves or creates outline colors',()=>{
 const document=compositionState.document
 const track=document.tracks[0] as TextTrack,clip=track.clips[0]!
 const base={...document,tracks:document.tracks.map(t=>t.id===track.id?{...track,clips:track.clips.map(c=>c.id===clip.id?{...c,colorAnimation:{mode:'constant' as const,value:'#AABBCCFF'},style:{...c.style,strokeWidthPx:2,strokeColor:'#123456FF'}}:c)}:t)}
 const result=restyleCaptions(base,{color:{r:10,g:20,b:30,a:128},outline_width:4})
 const changed=(result.document.tracks[0] as TextTrack).clips[0]!
 expect(changed.colorAnimation).toBeUndefined()
 expect(changed.style).toMatchObject({color:'#0a141e80',strokeWidthPx:4,strokeColor:'#123456FF'})
 expect((result.document.tracks[1] as TextTrack).clips[0]!.style.strokeColor).toBe('#000000FF')
 const removed=restyleCaptions(result.document,{outline_width:0})
 const added=restyleCaptions(removed.document,{outline_width:3})
 expect((added.document.tracks[0] as TextTrack).clips[0]!.style.strokeColor).toBe('#000000FF')
})
it('refuses empty/invalid patches, missing subjects and any locked lane atomically',()=>{
 const document=compositionState.document,before=JSON.stringify(document)
 for(const patch of [{},{font_size_px:null},{font_size_px:0},{outline_width:-1},{color:{r:256,g:0,b:0,a:255}}]) expect(()=>restyleCaptions(document,patch)).toThrow()
 expect(()=>restyleCaptions(document,{font_size_px:20},[])).toThrow()
 expect(()=>restyleCaptions(document,{font_size_px:20},['missing'])).toThrow()
 const locked={...document,tracks:document.tracks.map((track,i)=>i===1?{...track,locked:true}:track)}
 expect(()=>restyleCaptions(locked,{font_size_px:20})).toThrow('заблокированы')
 expect(JSON.stringify(document)).toBe(before)
})
it('restyles every authored nested composition once, including unused compositions',()=>{
 const wire=buildCompositionWireDocument(compositionState.document)
 const contents=Object.fromEntries(['root','child','unused'].map(id=>[id,{content:{...wire,tracks:wire.tracks.map(track=>({...track,clips:track.clips.map(clip=>({...clip,id:`${id}-${clip.id}`}))})) as typeof wire.tracks},instances:[]}]))
 const project:NativeNestedProject={schemaVersion:1,root:'root',compositions:contents}
 const before=JSON.stringify(project)
 const result=restyleNestedCaptions(project,{font_size_px:66,color:{r:255,g:0,b:0,a:255}})
 expect(result).toMatchObject({captions:9,restyled:9})
 for(const composition of Object.values(result.document.compositions)) for(const track of composition.content.tracks) if(track.kind==='text') for(const clip of track.clips) expect(clip.style).toMatchObject({fontSize:66,color:{red:1,green:0,blue:0,alpha:1}})
 const unused=project.compositions.unused!.content.tracks[0]!.clips[0]!.id
 expect(restyleNestedCaptions(project,{font_size_px:88},[unused])).toMatchObject({captions:9,restyled:1})
 const locked={...project,compositions:{...project.compositions,unused:{...project.compositions.unused!,content:{...project.compositions.unused!.content,tracks:project.compositions.unused!.content.tracks.map(track=>({...track,locked:true}))}}}}
 expect(()=>restyleNestedCaptions(locked,{font_size_px:20})).toThrow()
 expect(JSON.stringify(project)).toBe(before)
})
