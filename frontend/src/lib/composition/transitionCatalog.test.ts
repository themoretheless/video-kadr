import {test,expect} from 'vitest'
import {TRANSITION_DEFINITIONS,defaultProceduralTransition,proceduralTransitionReason} from './transitionCatalog'
import {boundaryTransition} from './boundaryTransition'
test('all 112 original and Concat transition controls validate and survive boundary edits',()=>{
 expect(TRANSITION_DEFINITIONS).toHaveLength(131)
 for(const definition of TRANSITION_DEFINITIONS){
  const kernel=defaultProceduralTransition(definition.id)
  expect(proceduralTransitionReason(kernel),definition.id).toBeNull()
  const original={...boundaryTransition('fade','a','b','procedural',1000000,2000000),kernel}
  expect(boundaryTransition('fade','a','b','procedural',1500000,2000000,original).kernel).toEqual(kernel)
 }
})
test('typed catalog rejects fallback, wrong parameter type and enum bounds',()=>{
 expect(proceduralTransitionReason({effect:'missing'})).not.toBeNull()
 expect(proceduralTransitionReason({effect:'block_motion',params:{direction:{Choice:4}}})).not.toBeNull()
 expect(proceduralTransitionReason({effect:'block_motion',params:{blocks:{Bool:true}}})).not.toBeNull()
 expect(proceduralTransitionReason({effect:'block_motion',params:{blocks:{Float:33}}})).not.toBeNull()
 expect(boundaryTransition('fade','a','b','procedural',1000000,2000000).kernel?.effect).toBe('cross_dissolve')
})

test('all procedural control sets survive validated project serialization and qualified export',async()=>{
 const {createComposition}=await import('./commands')
 const {buildCompositionWireDocument,buildCompositionRenderRequest}=await import('./payload')
 const {normalizeComposition}=await import('./validation')
 for(const definition of TRANSITION_DEFINITIONS){
  const transition={...boundaryTransition('fade','a','b','procedural',500000,1000000),kernel:defaultProceduralTransition(definition.id)}
  const c={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video' as const,durationTicks:4000000,width:320,height:180,hasAudio:false}},tracks:[{id:'video',kind:'video' as const,name:'Video',hidden:false,muted:false,locked:false,clips:['a','b'].map((id,index)=>({id,kind:'video' as const,sourceId:'source',timelineStartTicks:index*1000000,sourceInTicks:1000000,sourceOutTicks:2000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const}})),transitions:[transition]}]}
  const restored=normalizeComposition(JSON.parse(JSON.stringify(c)))
  const wire=buildCompositionWireDocument(restored)
  expect(wire.tracks[0]).toMatchObject({transitions:[{kind:'procedural',kernel:transition.kernel}]})
  expect(buildCompositionRenderRequest(c).composition).toEqual(wire)
 }
})
