import {expect,it} from 'vitest'
import {ordinaryAffected,nestedAffected} from './affectedEntities'
import {createComposition} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
import type {Composition,TextClip} from '../composition/types'
const empty=createComposition({width:320,height:180,fps:25,backgroundColor:'#000000FF'})
const clip:TextClip={id:'caption',kind:'text',text:'До удаления',timelineStartTicks:0,durationTicks:1000000,x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center'}}
const document:Composition={...empty,tracks:[{id:'track',kind:'text',name:'Субтитры',hidden:false,locked:false,clips:[clip]}]}
it('retains deleted labels, detects movement and renaming, and ignores unchanged objects',()=>{
 expect(ordinaryAffected(document,document)).toEqual([])
 expect(ordinaryAffected(document,empty)).toEqual(expect.arrayContaining([{kind:'Layer',id:'caption',compositionId:'ordinary',label:'До удаления'},{kind:'Track',id:'track',compositionId:'ordinary',label:'Субтитры'}]))
 const moved:Composition={...document,tracks:[{...document.tracks[0]!,clips:[]},{id:'destination',kind:'text',name:'Destination',hidden:false,locked:false,clips:[clip]}]}
 expect(ordinaryAffected(document,moved).filter(item=>item.kind==='Layer')).toEqual([{kind:'Layer',id:'caption',compositionId:'ordinary',label:'До удаления'}])
 const renamed={...document,metadata:{clipNames:{caption:'Новое имя'}}}
 expect(ordinaryAffected(document,renamed)).toContainEqual({kind:'Layer',id:'caption',compositionId:'ordinary',label:'Новое имя'})
})
it('keeps native repeated IDs scoped to the changed composition',()=>{
 const wire=buildCompositionWireDocument(document)
 const before={schemaVersion:1 as const,root:'root',compositions:{root:{content:wire,instances:[]},child:{content:wire,instances:[]}}}
 const after={...before,compositions:{...before.compositions,child:{content:buildCompositionWireDocument(empty),instances:[]}}}
 const affected=nestedAffected(before,after)
 expect(affected.filter(item=>item.kind==='Layer')).toEqual([{kind:'Layer',id:'caption',compositionId:'child',label:'До удаления'}])
 expect(affected.some(item=>item.compositionId==='root')).toBe(false)
})
it('bounds and redacts retained labels without retaining whole clip payloads',()=>{
 const changed={...document,metadata:{clipNames:{caption:'Bearer hidden-token '+ 'я'.repeat(1000)}}}
 const entity=ordinaryAffected(document,changed).find(item=>item.kind==='Layer')!
 expect(entity.label.length).toBeLessThanOrEqual(128)
 expect(entity.label).not.toContain('hidden-token')
 expect(Object.keys(entity).sort()).toEqual(['compositionId','id','kind','label'])
})

it('reports changed media bindings even when the timeline document is unchanged',()=>{
 expect(ordinaryAffected(empty,empty,{source:{filename:'before.mp4'}},{source:{filename:'after.mp4'}})).toEqual([{kind:'Media',id:'source',compositionId:'ordinary',label:'after.mp4'}])
})
