import {afterEach,expect,it,vi} from 'vitest'
import {inspectNativeTemplate,compositionProjectTemplateUrl,type NativeNestedProject} from './api'
const source={id:'slot',kind:'image' as const,durationTicks:0,width:16,height:16,hasAudio:false}
// Metadata-only response validation does not inspect a timeline's rendering model.
const project={schemaVersion:1,root:'root',compositions:{},projectMedia:{slot:{source,name:'Slot',placeholder:true}}} as NativeNestedProject
const info={schemaVersion:1,compositionId:'active',width:16,height:16,slots:[{mediaId:'slot',name:'Slot',kind:'image',seconds:2.5}]}
afterEach(()=>vi.unstubAllGlobals())
it('asks Rust for the selected timeline and retains the ordered slot metadata',async()=>{
 const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify(info)));vi.stubGlobal('fetch',fetcher)
 const controller=new AbortController();expect(await inspectNativeTemplate(project,'active',controller.signal)).toEqual(info)
 expect(fetcher.mock.calls[0][0]).toContain('/api/compositions/nested-template-info')
 expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({project,compositionId:'active'});expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
})
it('refuses missing, duplicated, mismatched or invalid-duration slot inventories',async()=>{
 for(const patch of [{compositionId:'other'},{slots:[]},{slots:[info.slots[0],info.slots[0]]},{slots:[{...info.slots[0],mediaId:'unknown'}]},{slots:[{...info.slots[0],kind:'audio'}]},{slots:[{...info.slots[0],seconds:-1}]}]){
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(new Response(JSON.stringify({...info,...patch}))))
  await expect(inspectNativeTemplate(project,'active')).rejects.toThrow('invalid slots')
 }
})
it('does not publish a response observed after cancellation',async()=>{
 const controller=new AbortController();vi.stubGlobal('fetch',vi.fn().mockImplementation(async()=>{controller.abort();return new Response(JSON.stringify(info))}))
 await expect(inspectNativeTemplate(project,'active',controller.signal)).rejects.toThrow('Aborted')
})

it('uses a streaming download URL with an encoded project identity',()=>{expect(compositionProjectTemplateUrl('project/one')).toBe('/api/composition-projects/project%2Fone/template')})
