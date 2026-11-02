import {expect,test} from 'vitest'
import {createComposition} from './commands'
import type {Composition,VideoTrack} from './types'
import {independentTransitionExportReason} from './transitionExport'
const document=():Composition=>({...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:3000000,width:320,height:180,hasAudio:true,audioChannels:2}},tracks:[{id:'video',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[{id:'a',kind:'video',sourceId:'source',timelineStartTicks:0,sourceInTicks:1000000,sourceOutTicks:2000000,speed:1,opacity:1,sourceAudioEnabled:true,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain'}}],transitions:[{id:'fade',kind:'film_dissolve',fromClipId:'a',toClipId:null,durationTicks:0,timeline:{startTick:'0',durationTick:'254016000000'}}]}]})
test('independent export admission supports native kernel and rejects side processing before rendering',()=>{
 const c=document();expect(independentTransitionExportReason(c)).toBeNull()
 const track=c.tracks[0] as VideoTrack
 const withEffect={...c,tracks:[{...track,clips:track.clips.map(clip=>({...clip,videoEffects:[{preset:'sharpen',intensity:1}] as const}))}]}
 expect(independentTransitionExportReason(withEffect)).toContain('Эффекты сторон')
 expect(independentTransitionExportReason({...c,tracks:[{...track,transitions:track.transitions!.map(t=>({...t,kind:'slide_left'}))}]})).toContain('не перенесён')
 const legacy={...track.transitions![0]!,id:'legacy',timeline:undefined,durationTicks:500000}
 expect(independentTransitionExportReason({...c,tracks:[{...track,transitions:[...track.transitions!,legacy]}]})).toContain('Совмещение')
 expect(independentTransitionExportReason({...c,tracks:[{...track,hidden:true}]})).toBeNull()
})


test('independent window keeps disabled video track primary until hidden', async () => {
 const {primaryCompositionVideoTrack} = await import('./validation')
 const c=document()
 const original=c.tracks[0] as VideoTrack
 const track={...original,clips:original.clips.map(clip=>({...clip,enabled:false}))}
 expect(primaryCompositionVideoTrack({...c,tracks:[track]})?.id).toBe('video')
 expect(primaryCompositionVideoTrack({...c,tracks:[{...track,hidden:true}]})).toBeNull()
 expect(primaryCompositionVideoTrack({...c,tracks:[{...track,transitions:[]}]})).toBeNull()
})

test('render request retains exact independent windows and disabled audio endpoints for all connected kernels',async()=>{
 const {buildCompositionRenderRequest}=await import('./payload')
 const kinds=['concat_dissolve','concat_fade_black','concat_fade_white','dissolve','film_dissolve','additive_dissolve','fade_black','fade_white','wipe_left','wipe_right','wipe_up','wipe_down'] as const
 for(const kind of kinds)for(const disabled of [false,true]){
  const c=document();const track=c.tracks[0] as VideoTrack
  const transition={...track.transitions![0]!,kind,reverse:true,timeline:{startTick:'-1',durationTick:'254016000001'}}
  const authored={...c,tracks:[{...track,clips:track.clips.map(clip=>({...clip,enabled:!disabled})),transitions:[transition]}]}
  const request=buildCompositionRenderRequest(authored)
  const wire=request.composition.tracks[0] as {clips:readonly {enabled:boolean}[],transitions:readonly unknown[]}
  expect(wire.transitions).toEqual([transition])
  expect(wire.clips[0]!.enabled).toBe(!disabled)
 }
})


test('render request admits independent transition with a separate DSP audio track',async()=>{
 const {buildCompositionRenderRequest}=await import('./payload')
 const c=document()
 const authored:Composition={...c,tracks:[...c.tracks,{id:'music',kind:'audio',name:'Music',muted:false,locked:false,clips:[{id:'music',kind:'audio',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,speed:1,gain:1,pan:0,dspEffects:[{id:'amplify',params:{gain:-6}}]}]}]}
 expect(independentTransitionExportReason(authored)).toBeNull()
 const request=buildCompositionRenderRequest(authored)
 expect(request.composition.tracks[1]).toMatchObject({kind:'audio',clips:[{dspEffects:[{id:'amplify',params:{gain:-6}}]}]})
})

test('visual preview admits procedural controls but preserves side-processing and mixed-window limits',async()=>{
 const {independentTransitionPreviewReason}=await import('./transitionExport')
 const {buildIndependentTransitionPreviewProject}=await import('./transitionPreview')
 const {defaultProceduralTransition}=await import('./transitionCatalog')
 const c=document();const original=c.tracks[0] as VideoTrack
 const track={...original,transitions:original.transitions!.map(t=>({...t,kind:'procedural' as const,kernel:defaultProceduralTransition('block_motion')}))}
 const authored={...c,tracks:[track]}
 expect(independentTransitionPreviewReason(authored)).toBeNull()
 expect(buildIndependentTransitionPreviewProject(authored)).not.toBeNull()
 const effected={...authored,tracks:[{...track,clips:track.clips.map(clip=>({...clip,videoEffects:[{preset:'sharpen',intensity:1}] as const}))}]}
 expect(()=>buildIndependentTransitionPreviewProject(effected)).toThrow('Эффекты сторон')
 const legacy={...original.transitions![0]!,id:'legacy',timeline:undefined,durationTicks:500000}
 expect(independentTransitionPreviewReason({...authored,tracks:[{...track,transitions:[...track.transitions,legacy]}]})).toContain('Совмещение')
 const overlay={...track,id:'overlay'}
 const primary={...track,id:'primary',clips:track.clips.map(clip=>({...clip,id:'primary-clip'})),transitions:[]}
 const layered={...authored,tracks:[overlay,primary]}
 expect(independentTransitionPreviewReason(layered)).toBeNull()
 expect(independentTransitionExportReason(layered)).toContain('overlay')
})
