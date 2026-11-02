import { expect, it } from 'vitest'
import { applyColorMatch } from './colorMatch'
import type { WireLumetri } from './types'
import type { NativeColorMatch } from '$lib/api'
const match: NativeColorMatch = {shadows:[0.1,-0.2],midtones:[0.3,0.4],highlights:[-0.1,0.2],lightness:[10,-20,30],saturation:120,before:0.2,after:0.01}
it('replaces solved channels, removes their animation and retains other settings',()=>{
 const key={mode:'constant' as const,value:5}
 const p:WireLumetri={enabled:false,wheelsOn:false,basic:{enabled:false,temperature:22},grade:{look:6,intensity:80},curves:{blue:[[0,0],[1,0.7]]},animation:{basic:{saturation:key,temperature:key},grade:{shadowsX:key,intensity:key},creative:{fadedFilm:key}}}
 const q=applyColorMatch(p,match)
 expect(q.enabled).toBe(true);expect(q.wheelsOn).toBe(true)
 expect(q.basic).toEqual({enabled:false,temperature:22,saturation:120})
 expect(q.grade?.shadows).toEqual({x:0.1,y:-0.2,lightness:10})
 expect(q.grade?.look).toBe(6);expect(q.curves).toEqual(p.curves)
 expect(q.animation?.basic).toEqual({temperature:key})
 expect(q.animation?.grade).toEqual({intensity:key})
 expect(q.animation?.creative).toEqual(p.animation?.creative)
 expect(p.animation?.basic?.saturation).toEqual(key)
})
it('rejects hostile output before an edit',()=>{
 expect(()=>applyColorMatch({}, {...match,saturation:NaN})).toThrow()
 expect(()=>applyColorMatch({}, {...match,shadows:[2,0]})).toThrow()
})
