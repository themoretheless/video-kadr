import {expect,it} from 'vitest'
import {createGraphicTemplateClip} from './graphicLayers'
import type {NativeGraphicTemplate} from '../api'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionRenderRequest} from './payload'
import {normalizeComposition} from './validation'
it('places a detached template with native canvas, control metadata, pins and roll and preserves its project roundtrip',()=>{
 const template:NativeGraphicTemplate={id:'builtin:test',resources:{fonts:[{family:'Test',style:'Regular',license:'OFL-1.1',file:'test.ttf',data:'TWFu'}]},name:'Test',category:'Titles',description:'Test',author:'Filmcraft',license:'MIT',tags:[],canvas:[1920,1080],durationUs:6000000,graphic:{layers:[{uid:2,enabled:true,originalParameters:{text:{value:{Text:'Привет'},keyframes:[{time:-254016000000,value:{Text:'Начало'},interp:'Hold'},{time:254016000000,value:{Text:'Конец'},interp:'Linear'}]}},content:{kind:'text',parameters:{text:'Привет'}}},{uid:1,enabled:true,pin:{to:{layer:2},left:true,right:true,top:false,bottom:false,offsets:[-4,0,4,0]},content:{kind:'shape',parameters:{size:[100,50]}}}]},roll:{mode:'roll',startOffScreen:true,endOffScreen:true,preroll:0,easeIn:0,easeOut:0,postroll:0},controls:[{id:'text',name:'Text',kind:'text',layer:2,param:'text'}]}
 const clip=createGraphicTemplateClip({width:128,height:72},template,'title',500000)
 expect(clip).toMatchObject({width:1920,height:1080,timelineStartTicks:500000,durationTicks:6000000,transform:{width:128,height:72},template:{id:template.id,controls:template.controls},graphic:template.graphic,roll:template.roll})
 expect(clip.graphic).not.toBe(template.graphic);expect(clip.graphic.layers[1]!.pin).not.toBe(template.graphic.layers[1]!.pin)
 let document=createComposition({width:128,height:72,fps:24,backgroundColor:'#000000'})
 document=addTrack(document,{id:'graphics',kind:'graphic',name:'Graphics',hidden:false,locked:false,clips:[]});document=addClip(document,'graphics',clip);document=normalizeComposition(document)
 expect(normalizeComposition(JSON.parse(JSON.stringify(document)))).toEqual(document)
 const wire=buildCompositionRenderRequest(document).composition
 expect(wire.sources).toEqual({});expect(wire.tracks[0]).toMatchObject({kind:'graphic',clips:[{graphic:template.graphic,roll:template.roll,template:clip.template,width:1920,height:1080}]})
})
