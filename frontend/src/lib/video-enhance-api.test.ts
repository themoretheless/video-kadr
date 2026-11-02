import {afterEach,expect,it,vi} from 'vitest'
import {startVideoEnhancement,saveEnhancedVideo,listVideoEnhancements} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('keeps video job and asset transport authenticated, abortable and separate from still routes',async()=>{
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{status:200,headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal
 await startVideoEnhancement('owner','source /?',2,signal);await saveEnhancedVideo('owner','job /?','project',signal);await listVideoEnhancements('owner','source /?','old /?',signal)
 expect(fetch.mock.calls.map(call=>String(call[0]))).toEqual(['/api/enhance/video-jobs','/api/enhance/video-jobs/job%20%2F%3F/asset','/api/enhance/video-jobs?sourceId=source+%2F%3F&cursor=old+%2F%3F'])
 for(const call of fetch.mock.calls){expect(new Headers(call[1]?.headers).get('authorization')).toBe('Bearer owner');expect(call[1]?.signal).toBe(signal)}
 expect(fetch.mock.calls.slice(0,2).map(call=>JSON.parse(String(call[1]?.body)))).toEqual([{sourceId:'source /?',factor:2},{projectId:'project'}])
})
