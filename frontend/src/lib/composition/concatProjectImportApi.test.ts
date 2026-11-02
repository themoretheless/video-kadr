import {afterEach,expect,it,vi} from 'vitest'
import {importConcatProject} from '$lib/api'
afterEach(()=>vi.unstubAllGlobals())
it('serializes the import contract with cancellation and the selected account bearer',async()=>{
 const report={schemaVersion:1,status:'needs_attention',project:null,diagnostics:['missing binding'],sourceDocument:{version:1},timelineIds:{}}
 const fetch=vi.fn().mockResolvedValue(new Response(JSON.stringify(report),{status:200}));vi.stubGlobal('fetch',fetch)
 const controller=new AbortController();expect(await importConcatProject({version:1},{},controller.signal,'selected-token')).toEqual(report)
 expect(fetch).toHaveBeenCalledOnce();const[path,init]=fetch.mock.calls[0]! as [string,RequestInit];expect(path).toBe('/api/compositions/concat-project/import');expect(new Headers(init.headers).get('Authorization')).toBe('Bearer selected-token');expect(init.signal).toBe(controller.signal);expect(JSON.parse(init.body as string)).toEqual({schemaVersion:1,document:{version:1},mediaBindings:{}})
})
it('sends explicitly selected font bytes without converting source paths into reads',async()=>{
 const fetch=vi.fn().mockResolvedValue(new Response(JSON.stringify({status:'needs_attention'}),{status:200}));vi.stubGlobal('fetch',fetch)
 const document={fonts:[{family:'Custom',path:'/never/open/font.ttf'}]},fontBindings={'/never/open/font.ttf':{data:'AQID'}}
 await importConcatProject(document,{},undefined,'selected-token',fontBindings)
 const[,init]=fetch.mock.calls[0]! as [string,RequestInit];expect(JSON.parse(init.body as string)).toEqual({schemaVersion:1,document,mediaBindings:{},fontBindings});expect(fetch).toHaveBeenCalledOnce()
})
