import { expect, it } from 'vitest'
import { decodeMonitorFrame } from './monitor-frame'
it('preserves straight RGBA including transparent colored pixels', () => {
 expect([...decodeMonitorFrame({width:2,height:1,ptsTicks:'9007199254740993',rgbaHex:'1F7fdf3fff000000'})]).toEqual([31,127,223,63,255,0,0,0])
})
it('rejects truncated, non-hex and oversized frames before allocation', () => {
 for (const patch of [{rgbaHex:'00000'},{rgbaHex:'zz000000'},{width:481},{height:271},{width:0},{width:1.5}]) expect(()=>decodeMonitorFrame({width:1,height:1,ptsTicks:'0',rgbaHex:'00000000',...patch})).toThrow('Некорректный')
})
