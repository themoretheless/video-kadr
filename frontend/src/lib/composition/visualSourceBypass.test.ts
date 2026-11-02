import {expect,it} from 'vitest'
import {createComposition,splitClip} from './commands'
import {assertValidComposition} from './validation'
import {buildCompositionWireDocument} from './payload'
import type {Composition,WireVideoEffect} from './types'
const bypass:WireVideoEffect={kind:'concat_bypass',link:{id:'unknown.saved.package',enabled:false,params:{custom:123},keys:{custom:[{at:0,value:-7,ease:[0,0,1,1]},{at:1,value:9,ease:[0,0,1,1]}]}}}
function fixture():Composition{return {...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:false}},tracks:[{kind:'video',id:'v',name:'Video',hidden:false,muted:false,locked:false,transitions:[],clips:[{kind:'video',id:'clip',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,sourceAudioEnabled:false,audioGain:1,opacity:1,transform:{x:0,y:0,width:16,height:16,fit:'contain'},effects:[bypass]}]}]}}
it('keeps opaque disabled source data and rewindows it on split',()=>{
 const input=fixture(),before=JSON.stringify(input);assertValidComposition(input)
 const wire=buildCompositionWireDocument(splitClip(input,'clip',500000,'tail'))
 const clips=wire.tracks[0]!.clips as readonly import('./types').WireVideoClip[]
 expect(clips.map(clip=>clip.effects)).toMatchObject([[{kind:'concat_bypass',link:{id:'unknown.saved.package',enabled:false,params:{custom:123},keys:{custom:[{value:-7},{value:1}]}}}],[{kind:'concat_bypass',link:{keys:{custom:[{value:1},{value:9}]}}}]])
 expect(JSON.stringify(input)).toBe(before)
 const invalid=JSON.parse(before);invalid.tracks[0].clips[0].effects[0].link.enabled=true;expect(()=>assertValidComposition(invalid)).toThrow()
})
