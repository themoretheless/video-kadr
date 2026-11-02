import type {NativeScalarKeySelection} from './nativeScalarTimeline'
import type {ScalarTimeView} from './nativeScalarViewport'

export interface NativeScalarHistoryView {
 context:string
 selection:NativeScalarKeySelection[]
 view:ScalarTimeView|null
}
export type NativeScalarHistoryViews=ReadonlyArray<{owner:symbol;value:NativeScalarHistoryView}>
type Provider={capture:()=>NativeScalarHistoryView;restore:(value:NativeScalarHistoryView)=>void}
const providers=new Map<symbol,Provider>()
const clone=(value:NativeScalarHistoryView):NativeScalarHistoryView=>({context:value.context,selection:value.selection.map(key=>({...key})),view:value.view?{...value.view}:null})

// UI state belongs to the mounted editor lifetime, never to the project wire document.
export function registerNativeScalarHistoryView(provider:Provider):()=>void{
 const owner=Symbol('native scalar history view');providers.set(owner,provider)
 return ()=>{providers.delete(owner)}
}
export function captureNativeScalarHistoryViews():NativeScalarHistoryViews{
 return [...providers].map(([owner,provider])=>({owner,value:clone(provider.capture())}))
}
export function restoreNativeScalarHistoryViews(views:NativeScalarHistoryViews=[]):void{
 for(const {owner,value} of views){const provider=providers.get(owner);if(provider&&provider.capture().context===value.context)provider.restore(clone(value))}
}
