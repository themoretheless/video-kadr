import {expect,it} from 'vitest'
import {scalarEdgePanSpeed,nativeScalarExtent,constrainScalarTimeView,zoomScalarTimeView,panScalarTimeView,scalarKeysInTimeRange,scalarKeysInBox,combineScalarKeySelection} from './nativeScalarViewport'
import {nativeScalarChannels} from './nativeScalarTimeline'
import {scalarClipboardProject} from './fixtures/nativeScalarClipboardProject'
it('uses clip extents and continuous local positions for mixed-clock range selection',()=>{
 const channels=nativeScalarChannels(scalarClipboardProject(),'root'),selected=scalarKeysInTimeRange(channels,1000000,1500000)
 expect(selected.map(key=>key.tick)).toEqual([250,750,1250,0,12]);expect(scalarKeysInTimeRange(channels,1500000,1000000)).toEqual(selected)
 expect(nativeScalarExtent(channels,[{start:2000000,end:4000000}])).toEqual({start:0,end:4000000})
 const x=channels[0]!,clocked={...x,value:{...x.value,track:{...x.value.track,timeBase:24,sampleClock:{rateNum:1,rateDen:1,offsetNum:0},keyframes:[{tick:1,value:0}]}}}
 expect(scalarKeysInTimeRange([clocked],1041666,1041667)).toHaveLength(1);expect(scalarKeysInTimeRange([clocked],1041666,1041666.5)).toHaveLength(0)
 expect(scalarKeysInTimeRange(channels.map(channel=>({...channel,locked:true})),0,9999999)).toEqual([])
})
it('combines range hits without aliases or duplicate identities in all four modes',()=>{
 const a={channelId:'a',tick:1},b={channelId:'b',tick:2},c={channelId:'c',tick:3}
 expect(combineScalarKeySelection([a,b],[b,c],'replace')).toEqual([b,c]);expect(combineScalarKeySelection([a,b],[b,c],'add')).toEqual([a,b,c]);expect(combineScalarKeySelection([a,b],[b,c],'toggle')).toEqual([a,c]);expect(combineScalarKeySelection([a,b],[b,c],'subtract')).toEqual([a])
 expect(combineScalarKeySelection([a],[a],'add')[0]).not.toBe(a)
 const points=[{key:a,x:10,y:10},{key:b,x:30,y:50},{key:c,x:50,y:90}]
 expect(scalarKeysInBox(points,40,60,0,0)).toEqual([a,b]);expect(()=>scalarKeysInBox(points,0,NaN,1,2)).toThrow('рамка')
})
it('zooms about a stable anchor, pans within extents and constrains invalid viewport requests',()=>{
 const extent={start:0,end:3000000},zoomed=zoomScalarTimeView(extent,extent,2,1500000)
 expect(zoomed).toEqual({start:750000,end:2250000});expect(zoomScalarTimeView(zoomed,extent,0.5,1500000)).toEqual(extent)
 expect(panScalarTimeView(zoomed,extent,99999999)).toEqual({start:1500000,end:3000000});expect(panScalarTimeView(zoomed,extent,-99999999)).toEqual({start:0,end:1500000})
 expect(constrainScalarTimeView({start:2000000,end:5000000},extent)).toEqual(extent);expect(()=>constrainScalarTimeView({start:2,end:1},extent)).toThrow('область');expect(()=>zoomScalarTimeView(extent,extent,0,1)).toThrow('масштаб')
 expect(zoomScalarTimeView(extent,extent,1e99,1500000).end-zoomScalarTimeView(extent,extent,1e99,1500000).start).toBe(1)
})

it('ramps edge pan speed within a bounded zone and rejects invalid screen geometry',()=>{
 expect(scalarEdgePanSpeed(150,0,300)).toBe(0);expect(scalarEdgePanSpeed(16,0,300)).toBe(-.5);expect(scalarEdgePanSpeed(284,0,300)).toBe(.5)
 expect(scalarEdgePanSpeed(-100,0,300)).toBe(-1);expect(scalarEdgePanSpeed(1000,0,300)).toBe(1)
 expect(scalarEdgePanSpeed(15,10,20)).toBe(0);expect(scalarEdgePanSpeed(29,10,20)).toBe(.8)
 expect(scalarEdgePanSpeed(NaN,0,300)).toBe(0);expect(scalarEdgePanSpeed(0,0,0)).toBe(0)
})
