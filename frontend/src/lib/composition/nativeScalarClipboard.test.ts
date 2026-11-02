import {expect,it} from 'vitest'
import {captureNativeScalarClipboard,planNativeScalarPaste,serializeNativeScalarClipboard,parseNativeScalarClipboard,MAX_SCALAR_CLIPBOARD_BYTES} from './nativeScalarClipboard'
import {nativeScalarChannels} from './nativeScalarTimeline'
import {scalarClipboardProject} from './fixtures/nativeScalarClipboardProject'
const all=(project:ReturnType<typeof scalarClipboardProject>)=>nativeScalarChannels(project,'root').flatMap(channel=>channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})))
it('copies detached composition-time spacing and promotes constant destination channels atomically',()=>{
 const project=scalarClipboardProject(),before=JSON.stringify(project),packet=captureNativeScalarClipboard(project,'root',all(project))
 expect(packet.groups.map(group=>group.keys.map(key=>key.tick))).toEqual([[0,250000,500000],[0,500000,1000000]])
 const text=serializeNativeScalarClipboard(packet);expect(serializeNativeScalarClipboard(parseNativeScalarClipboard(text))).toBe(text)
 const plan=planNativeScalarPaste(project,'root',packet,'target',2250000),channels=nativeScalarChannels(plan.project,'root').filter(channel=>channel.clipId==='target')
 expect(channels.find(channel=>channel.property==='x')!.value.track.keyframes.map(key=>[key.tick,key.value])).toEqual([[250000,0],[500000,20],[750000,40]])
 expect(channels.find(channel=>channel.property==='opacity')!.value.track.keyframes.map(key=>key.tick)).toEqual([250000,750000,1250000]);expect(plan.selection).toHaveLength(6);expect(plan.skipped).toEqual([])
 expect(JSON.stringify(project)).toBe(before)
 const source=project.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in source))throw new Error('Visual required');Object.assign(source.transform,{x:{mode:'constant',value:999}})
 expect(serializeNativeScalarClipboard(packet)).toBe(text)
})
it('retains destination sample clock and extrapolation while replacing exact-time collisions pasted-last',()=>{
 const project=scalarClipboardProject(),packet=captureNativeScalarClipboard(project,'root',all(project)),target=project.compositions.root!.content.tracks[1]!.clips[0]!;if(!('transform' in target))throw new Error('Visual required')
 Object.assign(target.transform,{x:{mode:'keyframes',track:{timeBase:1000,sampleClock:{rateNum:3,rateDen:2,offsetNum:-100000},extrapolation:{before:'continue',after:'ping_pong'},interpolation:'linear',keyframes:[{tick:-50,value:80},{tick:325,value:999}]}}})
 const changed=planNativeScalarPaste(project,'root',packet,'target',2250000),x=nativeScalarChannels(changed.project,'root').find(channel=>channel.clipId==='target'&&channel.property==='x')!.value.track
 expect(x.sampleClock).toEqual({rateNum:3,rateDen:2,offsetNum:-100000});expect(x.extrapolation).toEqual({before:'continue',after:'ping_pong'});expect(x.keyframes.map(key=>[key.tick,key.value])).toEqual([[-50,80],[325,0],[700,20],[1075,40]])
})
it('materializes an inherited incoming Bezier handle before copying an isolated key',()=>{
 const project=scalarClipboardProject(),source=project.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in source))throw new Error('Visual required')
 Object.assign(source.transform,{x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:0,value:0,interpolation:{kind:'bezier',p1:[0.2,0.1],p2:[0.8,0.9]}},{tick:1000,value:20,interpolation:'hold'},{tick:2000,value:40}]}}})
 const x=nativeScalarChannels(project,'root').find(channel=>channel.property==='x')!,packet=captureNativeScalarClipboard(project,'root',[{channelId:x.id,tick:1000}])
 expect(packet.groups[0]!.keys[0]).toMatchObject({interpolation:'hold',tangents:{incoming:{x:0.8,y:0.9,mode:'free'},outgoing:{x:1/3,y:1/3,mode:'free'},continuity:'broken'}})
 const pasted=planNativeScalarPaste(project,'root',packet,'target',2250000);expect(nativeScalarChannels(pasted.project,'root').find(channel=>channel.clipId==='target')!.value.track.keyframes[0]!.tangents?.incoming).toEqual({x:0.8,y:0.9,mode:'free'})
})
it('refuses overrun, target locks and quantization collapse before changing any channel',()=>{
 const project=scalarClipboardProject(),packet=captureNativeScalarClipboard(project,'root',all(project)),before=JSON.stringify(project)
 expect(()=>planNativeScalarPaste(project,'root',packet,'target',3500000)).toThrow('конец');expect(JSON.stringify(project)).toBe(before)
 const target=project.compositions.root!.content.tracks[1]!.clips[0]!;if(!('transform' in target))throw new Error('Visual required');Object.assign(target.transform,{x:{mode:'keyframes',track:{timeBase:1,interpolation:'linear',keyframes:[{tick:0,value:80}]}}})
 expect(()=>planNativeScalarPaste(project,'root',packet,'target',2000000)).toThrow('объединяет')
 Object.assign(project.compositions.root!.content.tracks[1]!,{locked:true});expect(()=>planNativeScalarPaste(project,'root',packet,'target',2250000)).toThrow('заблокирована')
})
it.each([
 '{}','{"format":"video-kadr/scalar-keys","version":2,"groups":[]}',
 JSON.stringify({format:'video-kadr/scalar-keys',version:1,groups:[{parameter:'["visual","x"]',keys:[{tick:0,value:1,url:'/private'}]}]}),
 JSON.stringify({format:'video-kadr/scalar-keys',version:1,groups:[{parameter:'["visual","x"]',keys:[{tick:0,value:1,interpolation:{kind:'bezier',p1:[-1,0],p2:[1,1]}}]}]}),
 JSON.stringify({format:'video-kadr/scalar-keys',version:1,groups:[{parameter:'["visual","x"]',keys:[{tick:0,value:1},{tick:0,value:2}]}]})
])('refuses malformed scalar clipboard text: %s',text=>{expect(()=>parseNativeScalarClipboard(text)).toThrow()})
it('rejects oversized transport before JSON parsing',()=>{expect(()=>parseNativeScalarClipboard(' '.repeat(MAX_SCALAR_CLIPBOARD_BYTES+1))).toThrow('слишком большой')})
