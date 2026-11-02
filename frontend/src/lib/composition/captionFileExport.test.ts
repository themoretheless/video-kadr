import {expect,it} from 'vitest'
import {captionFileDocument} from './captionFileExport'
it('converts source ticks beyond Number safe precision without rounding',()=>{
 const document=captionFileDocument({id:'track',name:'Track',clips:[{id:'internal',timelineStartTick:36_000_000_001,timelineEndTick:36_000_000_002,text:'Caption'}]})
 expect(document.cues[0].startTick).toBe('9144576000254016')
 expect(document.cues[0].endTick).toBe('9144576000508032')
 expect(document.cues[0].id).toBeNull()
})
it('refuses times outside signed file clock without mutating authored track',()=>{
 const track={id:'track',name:'Track',clips:[{id:'cue',timelineStartTick:0,timelineEndTick:Number.MAX_SAFE_INTEGER,text:'Caption'}]}
 const snapshot=JSON.stringify(track)
 expect(()=>captionFileDocument(track)).toThrow('вне диапазона')
 expect(JSON.stringify(track)).toBe(snapshot)
})
it('exports authored fractional source ticks unchanged',()=>{
 const document=captionFileDocument({id:'track',name:'Track',clips:[{id:'cue',cueId:'file',timelineStartTick:'137729592000',timelineEndTick:'137729592001',text:'Exact'}]})
 expect(document.cues[0]).toMatchObject({startTick:'137729592000',endTick:'137729592001',id:'file'})
})
