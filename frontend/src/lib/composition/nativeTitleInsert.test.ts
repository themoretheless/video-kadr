import {expect,it} from 'vitest'
import {insertNativeConcatTitle} from './nativeTitleInsert'
import {MAX_COMPOSITION_DURATION_TICKS} from './types'
import type {NativeNestedProject} from '$lib/api'
const project=():NativeNestedProject=>({schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:320,height:180,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}})
const request={clipId:'title',trackId:'',newTrackId:'titles',text:'Новый\nTitle',startUs:500000,durationUs:2000000}
it('creates a source-free retained title on a new top lane and extends its composition',()=>{
 const input=project(),before=JSON.stringify(input),output=insertNativeConcatTitle(input,'root',request),node=output.compositions.root
 expect(JSON.stringify(input)).toBe(before);expect(node.durationUs).toBe(2500000);expect(node.content.sources).toEqual({});expect(node.content.tracks[0]).toMatchObject({id:'titles',kind:'text',clips:[{id:'title',text:request.text,concatTitle:{},timelineStartTick:500000,timelineEndTick:2500000}]})
 const next=insertNativeConcatTitle(output,'root',{...request,clipId:'second',trackId:'titles',startUs:2500000});expect(next.compositions.root.content.tracks).toHaveLength(1);expect(next.compositions.root.content.tracks[0].clips).toHaveLength(2)
})
it('refuses locks, duplicate IDs, invalid words/clocks and caption-owned tracks without mutations',()=>{
 const output=insertNativeConcatTitle(project(),'root',request),before=JSON.stringify(output)
 for(const patch of [{clipId:'title'},{clipId:'bad',startUs:-1},{clipId:'bad',durationUs:0},{clipId:'bad',startUs:MAX_COMPOSITION_DURATION_TICKS},{clipId:'bad',text:' '},{clipId:'bad',text:'🙂'.repeat(513)}])expect(()=>insertNativeConcatTitle(output,'root',{...request,...patch})).toThrow()
 const locked=structuredClone(output);(locked.compositions.root.content.tracks[0] as {locked:boolean}).locked=true;expect(()=>insertNativeConcatTitle(locked,'root',{...request,clipId:'other',trackId:'titles',startUs:3000000})).toThrow('заблокирована')
 const caption=structuredClone(output);caption.compositions.root.content={...caption.compositions.root.content,metadata:{captionTextTracks:['titles']}};expect(()=>insertNativeConcatTitle(caption,'root',{...request,clipId:'other',trackId:'titles',startUs:3000000})).toThrow('субтитров');expect(JSON.stringify(output)).toBe(before)
})
