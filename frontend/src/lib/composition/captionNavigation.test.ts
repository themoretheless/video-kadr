import {expect,it} from 'vitest'
import {captionNavigationTarget} from './captionNavigation'
import {createComposition} from './commands'
it('uses strict cue starts across visible tracks, including locked tracks, without mutations',()=>{
 const composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[
  {id:'hidden',name:'Hidden',hidden:true,clips:[{id:'hidden-cue',timelineStartTick:100,timelineEndTick:150,text:'Hidden'}]},
  {id:'visible',name:'Visible',locked:true,clips:[{id:'a',timelineStartTick:0,timelineEndTick:100,text:'First'},{id:'b',timelineStartTick:200,timelineEndTick:300,text:'Second'}]},
 ]}
 const before=JSON.stringify(composition)
 expect(captionNavigationTarget(composition,0,true).id).toBe('b')
 expect(captionNavigationTarget(composition,200,false).id).toBe('a')
 expect(captionNavigationTarget(composition,250,false).id).toBe('b')
 expect(()=>captionNavigationTarget(composition,200,true)).toThrow('Больше нет')
 expect(()=>captionNavigationTarget(composition,0,false)).toThrow('Больше нет')
 expect(JSON.stringify(composition)).toBe(before)
})
