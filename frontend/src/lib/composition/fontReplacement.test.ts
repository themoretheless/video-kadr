import {expect,it} from 'vitest'
import {applyFontReplacement} from './fontReplacement'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import type {GraphicClip} from './types'
it('applies native font fields without changing editor timing, transforms or layer animation',()=>{
 let document=createComposition({width:80,height:64,fps:24,backgroundColor:'#000000'})
 document=addTrack(document,{id:'g',kind:'graphic',name:'Graphics',hidden:false,locked:false,clips:[]})
 const clip:GraphicClip={id:'title',kind:'graphic',timelineStartTicks:123456,durationTicks:1000000,width:80,height:64,animationOffsetUs:42,opacity:1,
 transform:{x:7,y:8,width:80,height:64,fit:'contain'},graphic:{layers:[{uid:1,enabled:true,content:{kind:'text',parameters:{text:'Title',font:'Inter',fontStyle:'Bold'},sourceTextAnimation:{timeBase:1000000,keyframes:[{tick:0,value:'Title'}]}}}]}}
 document=addClip(document,'g',clip)
 const before=JSON.stringify(document),wire=structuredClone(buildCompositionWireDocument(document))
 const track=wire.tracks[0];if(track.kind!=='graphic')throw new Error('fixture')
 const layer=track.clips[0].graphic.layers[0]
 if(layer.content.kind!=='text')throw new Error('fixture text')
 const changed={...wire,tracks:[{...track,clips:[{...track.clips[0],graphic:{layers:[{...layer,content:{...layer.content,parameters:{...layer.content.parameters,font:'External',fontStyle:'Regular'}}}]}}]}]}
 const result=applyFontReplacement(document,changed)
 expect(JSON.stringify(document)).toBe(before)
 const resultClip=result.tracks[0].clips[0] as GraphicClip
 expect(resultClip.transform).toEqual(clip.transform);expect(resultClip.animationOffsetUs).toBe(42);expect(resultClip.timelineStartTicks).toBe(123456)
 expect(resultClip.graphic.layers[0].content).toEqual({...clip.graphic.layers[0].content,parameters:{text:'Title',font:'External',fontStyle:'Regular',runs:undefined}})
})
