import {afterEach,expect,it,vi} from 'vitest'
import {decodeScopePreferences,readScopePreferences,writeScopePreferences,SCOPE_PREFERENCES_KEY} from './scopePreferences'
afterEach(()=>vi.restoreAllMocks())
it('preserves valid fields and rejects obsolete or malformed settings independently',()=>{
 const result=decodeScopePreferences({kind:'vectorscopeHls',scale:'unknown',clamp:false,matrix:'unknown',targets:'100',brightness:'bright',peaks:999999})
 expect(result).toMatchObject({kind:'vectorscopeHls',scale:'bits8',clamp:false,matrix:'auto',targets:'100',brightness:'bright'})
 expect(result).not.toHaveProperty('peaks')
 expect(decodeScopePreferences(null)).toEqual(decodeScopePreferences([]))
})
it('round trips settings and tolerates denied and corrupt storage',()=>{
 const data=new Map<string,string>()
 vi.spyOn(window,'localStorage','get').mockReturnValue({getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>{data.set(key,value)}} as Storage)
 const selected={...decodeScopePreferences({}),kind:'parade' as const,scale:'float' as const,clamp:false}
 writeScopePreferences(selected);expect(readScopePreferences()).toEqual(selected)
 data.set(SCOPE_PREFERENCES_KEY,'{');expect(readScopePreferences().scale).toBe('bits8')
 data.set(SCOPE_PREFERENCES_KEY,' '.repeat(4097));expect(readScopePreferences().kind).toBe('waveform')
 vi.spyOn(window,'localStorage','get').mockImplementation(()=>{throw new Error('Denied')})
 expect(()=>writeScopePreferences(selected)).not.toThrow();expect(readScopePreferences().clamp).toBe(true)
})
it('bounds and deduplicates saved scope layouts without accepting an empty selection',()=>{
 expect(decodeScopePreferences({shown:['histogram','histogram','invalid','waveform']}).shown).toEqual(['histogram','waveform'])
 expect(decodeScopePreferences({shown:[]}).shown).toBeUndefined()
 expect(decodeScopePreferences({shown:Array(100).fill('waveform')}).shown).toBeUndefined()
})
