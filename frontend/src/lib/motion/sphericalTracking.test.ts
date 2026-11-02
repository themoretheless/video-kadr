import {it,expect} from 'vitest'
import {trackSphericalSubject} from './sphericalTracking'
function frame(center:number){const width=64,height=32,data=new Uint8Array(width*height);for(let dy=-3;dy<=3;dy++)for(let dx=-3;dx<=3;dx++){const x=(center+dx+width)%width;data[(16+dy)*width+x]=30+((dx+4)*61+(dy+4)*37+(dx+4)*(dy+4)*17)%225}return {width,height,data}}
it('tracks selected texture through longitude seam without a 360 degree spin',()=>{
 const result=trackSphericalSubject([frame(62),frame(63),frame(0),frame(1)],[0,.1,.2,.3],[62/64,.5])
 expect(result.pan.mode).toBe('keyframes');if(result.pan.mode!=='keyframes')throw Error('keys')
 const keys=result.pan.track.keyframes;expect(keys).toHaveLength(4)
 for(let i=1;i<keys.length;i++)expect(keys[i]!.value-keys[i-1]!.value).toBeCloseTo(360/64,9)
 expect(keys[3]!.tick).toBe(300000)
})
it('fails without partial camera keys on lost texture or invalid times',()=>{
 const empty={...frame(0),data:new Uint8Array(64*32)}
 expect(()=>trackSphericalSubject([frame(32),empty],[0,.1],[.5,.5])).toThrow(/потеряна/)
 expect(()=>trackSphericalSubject([frame(32),frame(33)],[0,0],[.5,.5])).toThrow()
})
it('reacquires the immutable selected texture after a short occlusion and interpolates only between measured endpoints',()=>{
 const blank={...frame(0),data:new Uint8Array(64*32)}
 const result=trackSphericalSubject([frame(20),frame(21),blank,blank,frame(32),frame(33)],[0,.2,.4,.6,.8,1],[20/64,.5])
 expect(result.recoveredGaps).toEqual([{fromSeconds:.2,toSeconds:.8}])
 if(result.pan.mode!=='keyframes')throw Error('keys')
 expect(result.pan.track.keyframes.map(k=>k.tick)).toEqual([0,200000,800000,1000000])
 expect(result.pan.track.keyframes[2]!.value).toBeCloseTo((32.5/64-.5)*360)
})
it('rejects ambiguous duplicate identities during global reacquisition',()=>{
 const blank={...frame(0),data:new Uint8Array(64*32)}
 const duplicates=frame(20);const second=frame(40)
 for(let i=0;i<duplicates.data.length;i++)duplicates.data[i]=Math.max(duplicates.data[i]!,second.data[i]!)
 expect(()=>trackSphericalSubject([frame(20),blank,duplicates],[0,.2,.4],[20/64,.5])).toThrow(/потеряна/)
})
it('refuses a persistent occlusion, invalid raster and low-texture seed',()=>{
 const blank={...frame(0),data:new Uint8Array(64*32)}
 expect(()=>trackSphericalSubject([frame(20),blank,frame(21)],[0,.1,2.5],[20/64,.5])).toThrow(/потеряна/)
 expect(()=>trackSphericalSubject([blank,blank],[0,.1],[.5,.5])).toThrow(/контрастный/)
 expect(()=>trackSphericalSubject([frame(20),frame(21)],[0,.1],[.5,1])).toThrow(/полюсов/)
 expect(()=>trackSphericalSubject([{...frame(20),width:64.5},frame(21)],[0,.1],[.5,.5])).toThrow(/размер/)
})
it('tracks an eligible sixty second interval within the bounded sample budget',()=>{
 const frames=Array.from({length:300},(_,i)=>frame((20+i)%64))
 const result=trackSphericalSubject(frames,frames.map((_,i)=>i/5),[20/64,.5])
 if(result.pan.mode!=='keyframes')throw Error('keys')
 expect(result.pan.track.keyframes).toHaveLength(300)
 expect(result.pan.track.keyframes[299]!.tick).toBe(59_800_000)
 expect(result.pan.track.keyframes[299]!.value-result.pan.track.keyframes[0]!.value).toBeCloseTo(299*360/64)
})
it('keeps texture identity through affine lighting changes',()=>{
 const changed=frame(21);changed.data=changed.data.map(v=>Math.round(v*.6+20))
 const result=trackSphericalSubject([frame(20),changed],[0,.2],[20/64,.5])
 if(result.pan.mode!=='keyframes')throw Error('keys')
 expect(result.pan.track.keyframes[1]!.value-result.pan.track.keyframes[0]!.value).toBeCloseTo(360/64)
})
