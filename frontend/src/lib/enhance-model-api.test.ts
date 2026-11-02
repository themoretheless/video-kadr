import {afterEach,expect,it,vi} from 'vitest'
import {listEnhancementModels,selectEnhancementModel,removeEnhancementModel,startEnhancementModelDownload,listEnhancementModelDownloads} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('uses authenticated model routes, exact JSON and encoded cursor with abort propagation',async()=>{
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{status:200,headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal
 await listEnhancementModels('owner',signal);await selectEnhancementModel('owner','realesr-general-x4v3',signal);await removeEnhancementModel('owner','realesr-general-x4v3',signal);await startEnhancementModelDownload('owner','realesr-general-x4v3',signal);await listEnhancementModelDownloads('owner','old /?&',signal)
 expect(fetch.mock.calls.map(call=>String(call[0]))).toEqual(['/api/enhance/models','/api/enhance/models/select','/api/enhance/models/remove','/api/enhance/models/downloads','/api/enhance/models/downloads?cursor=old%20%2F%3F%26'])
 for(const call of fetch.mock.calls){const options=call[1] as RequestInit;expect(new Headers(options.headers).get('authorization')).toBe('Bearer owner');expect(options.signal).toBe(signal)}
 expect(fetch.mock.calls.slice(1,4).map(call=>(call[1] as RequestInit).method)).toEqual(['POST','POST','POST'])
 expect(fetch.mock.calls.slice(1,4).map(call=>JSON.parse(String((call[1] as RequestInit).body)))).toEqual([{modelId:'realesr-general-x4v3'},{modelId:'realesr-general-x4v3'},{catalogId:'realesr-general-x4v3'}])
})
