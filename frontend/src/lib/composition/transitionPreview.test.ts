import {test,expect} from 'vitest'
import {TRANSITION_DEFINITIONS,defaultProceduralTransition} from './transitionCatalog'
import {boundaryTransition} from './boundaryTransition'
test('all 112 independent kernels build native preview documents and qualified file export',async()=>{
 const {createComposition}=await import('./commands')
 const {buildCompositionWireDocument,buildCompositionRenderRequest}=await import('./payload')
 const {normalizeComposition}=await import('./validation')
 const {buildIndependentTransitionPreviewProject}=await import('./transitionPreview')
 for(const definition of TRANSITION_DEFINITIONS){
  const transition={...boundaryTransition('fade','a','b','procedural',500000,1000000),kernel:defaultProceduralTransition(definition.id)}
  const c={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video' as const,durationTicks:4000000,width:320,height:180,hasAudio:false}},tracks:[{id:'video',kind:'video' as const,name:'Video',hidden:false,muted:false,locked:false,clips:['a','b'].map((id,index)=>({id,kind:'video' as const,sourceId:'source',timelineStartTicks:index*1000000,sourceInTicks:1000000,sourceOutTicks:2000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const}})),transitions:[transition]}]}
  const restored=normalizeComposition(JSON.parse(JSON.stringify(c)))
  const wire=buildCompositionWireDocument(restored)
  expect(wire.tracks[0]).toMatchObject({transitions:[{kind:'procedural',kernel:transition.kernel}]})
  expect(buildCompositionRenderRequest(c).composition).toEqual(wire)
  expect(buildIndependentTransitionPreviewProject(restored)?.compositions.editor?.content).toEqual(wire)
  expect(buildIndependentTransitionPreviewProject({...restored,tracks:restored.tracks.map(track=>({...track,hidden:true}))})).toBeNull()
 }
})
test('Concat raw sound overlap survives native preview/export normalization and rejects malformed metadata',async()=>{
 const {createComposition}=await import('./commands'),{normalizeComposition,validateComposition}=await import('./validation'),{buildCompositionWireDocument}=await import('./payload')
 const concatOverlap={leadTicks:250000,durationTicks:500000,holdOutgoing:true,authoredDurationTicks:2000000,legacy:'cross-fade' as const},transition={id:'cut',fromClipId:'a',toClipId:'b',kind:'concat_dissolve' as const,durationTicks:0,timeline:{startTick:750000,durationTick:500000},concatOverlap}
 const c={...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video' as const,durationTicks:4000000,width:16,height:16,hasAudio:true,audioChannels:2}},tracks:[{id:'v',kind:'video' as const,name:'Video',hidden:false,muted:false,locked:false,clips:['a','b'].map((id,index)=>({id,kind:'video' as const,sourceId:'source',timelineStartTicks:index*1000000,sourceInTicks:1000000,sourceOutTicks:2000000,speed:1,opacity:1,sourceAudioEnabled:true,audioGain:1,transform:{x:0,y:0,width:16,height:16,fit:'contain' as const}})),transitions:[transition]}]}
 for(const legacy of ['push','zoom','wipe-left','wipe-right','fade-black','fade-white'] as const){const movement={...c,tracks:[{...c.tracks[0]!,transitions:[{...transition,concatOverlap:{...concatOverlap,legacy}}]}]};expect(validateComposition(movement).some(issue=>issue.code==='transition-concat-overlap')).toBe(false);expect(buildCompositionWireDocument(normalizeComposition(JSON.parse(JSON.stringify(movement)))).tracks[0]).toMatchObject({transitions:[{concatOverlap:{legacy}}]})}
 const differentKernel={...c,tracks:[{...c.tracks[0]!,transitions:[{...transition,kind:'concat_fade_black' as const}]}]};expect(validateComposition(differentKernel).some(issue=>issue.code==='transition-concat-overlap')).toBe(true)
 const restored=normalizeComposition(JSON.parse(JSON.stringify(c)));expect(buildCompositionWireDocument(restored).tracks[0]).toMatchObject({transitions:[{concatOverlap}]})
 for(const invalid of [{leadTicks:-1,durationTicks:500000},{leadTicks:600000,durationTicks:500000},{leadTicks:0,durationTicks:0},{...concatOverlap,unknown:true},{...concatOverlap,holdOutgoing:'true'},{...concatOverlap,authoredDurationTicks:1},{...concatOverlap,legacy:true},{...concatOverlap,legacy:'unknown-transition'}]){const broken={...c,tracks:[{...c.tracks[0]!,transitions:[{...transition,concatOverlap:invalid}]}]};expect(validateComposition(broken).some(issue=>issue.code==='transition-concat-overlap')).toBe(true)}
})
