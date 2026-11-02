import {expect,it} from 'vitest'
import {captureActivityViewport,restoreActivityViewport,retainActivityExpansion,initialActivityViewport} from './activityViewport'
it('follows the bottom only within the threshold and can explicitly return to the latest calls',()=>{
 const metrics={scrollTop:780,scrollHeight:1000,clientHeight:200}
 const following=captureActivityViewport(metrics,[])
 expect(following.following).toBe(true)
 expect(restoreActivityViewport(following,{...metrics,scrollHeight:1200},[])).toBe(1000)
 expect(captureActivityViewport({...metrics,scrollTop:776},[]).following).toBe(false)
 expect(restoreActivityViewport(initialActivityViewport(),metrics,[])).toBe(800)
})
it('preserves a partially visible row when older records or details above it change height',()=>{
 const saved=captureActivityViewport({scrollTop:400,scrollHeight:1000,clientHeight:200},[{id:'above',top:-100,bottom:-1},{id:'first',top:-10,bottom:40},{id:'second',top:40,bottom:100},{id:'below',top:201,bottom:250}])
 expect(saved.anchors.map(item=>item.id)).toEqual(['first','second'])
 expect(restoreActivityViewport(saved,{scrollTop:400,scrollHeight:1300,clientHeight:200},[{id:'first',top:290,bottom:340},{id:'second',top:340,bottom:400}])).toBe(700)
})
it('uses a surviving visible fallback after eviction and clamps safely if all anchors disappear',()=>{
 const saved=captureActivityViewport({scrollTop:400,scrollHeight:1000,clientHeight:200},[{id:'evicted',top:-10,bottom:40},{id:'survivor',top:40,bottom:100}])
 expect(restoreActivityViewport(saved,{scrollTop:400,scrollHeight:950,clientHeight:200},[{id:'survivor',top:-10,bottom:50}])).toBe(350)
 expect(restoreActivityViewport(saved,{scrollTop:0,scrollHeight:300,clientHeight:200},[])).toBe(100)
 expect(restoreActivityViewport(saved,{scrollTop:0,scrollHeight:20,clientHeight:200},[])).toBe(0)
})
it('prunes expansion only against retained owner IDs, retaining hidden-by-filter rows',()=>{
 expect(retainActivityExpansion({'call:old':true,'reads:old':false,'session:old':true,'call:retained':true,'reads:retained':true,'session:retained':false},['retained'])).toEqual({'call:retained':true,'reads:retained':true,'session:retained':false})
})
