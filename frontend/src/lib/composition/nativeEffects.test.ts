import {expect,it} from 'vitest'
import {rebaseNativeEffects} from './nativeEffects'
import {sampleAnimatableValue} from './keyframes'
import type {WireVideoEffect,WireAnimatableValue} from './types'
const curve:WireAnimatableValue={mode:'keyframes',track:{timeBase:1000,interpolation:'ease_in_out_cubic',extrapolation:{before:'continue',after:'hold'},keyframes:[{tick:0,value:0.25},{tick:1000,value:0.75},{tick:2000,value:0.5}]}}
const constant=(value:number):WireAnimatableValue=>({mode:'constant',value})
it('rebases masks and nested Lumetri channels with independent copies and original curve metadata',()=>{
 const effects:readonly WireVideoEffect[]=[
  {kind:'mask',shape:'ellipse',x:curve,y:constant(0.5),width:curve,height:constant(1),rotationDegrees:constant(0),feather:0.1,inverted:true},
  {kind:'lumetri',parameters:{basic:{exposure:1},animation:{basic:{exposure:curve}}}},
 ]
 const before=JSON.stringify(effects)
 const shifted=rebaseNativeEffects(effects,-500000)!
 const mask=shifted[0]!,lumetri=shifted[1]!
 if(mask.kind!=='mask'||lumetri.kind!=='lumetri')throw new Error('effects')
 expect(mask.x).toMatchObject({track:{timeBase:1000,interpolation:'ease_in_out_cubic',extrapolation:curve.mode==='keyframes'?curve.track.extrapolation:undefined,keyframes:[{tick:-500,value:0.25},{tick:500,value:0.75},{tick:1500,value:0.5}]}})
 const exposure=lumetri.parameters.animation!.basic!.exposure!
 expect(exposure).toEqual(mask.x)
 for(const time of [0,100000,500000,900000,1500000]){
  expect(sampleAnimatableValue(mask.x,time)).toBe(sampleAnimatableValue(curve,time+500000))
  expect(sampleAnimatableValue(exposure,time)).toBe(sampleAnimatableValue(curve,time+500000))
 }
 expect(JSON.stringify(effects)).toBe(before)
 expect(mask.x).not.toBe(curve);expect(exposure).not.toBe(mask.x)
 const copied=rebaseNativeEffects(effects)!
 expect(copied).toEqual(effects);expect(copied[0]).not.toBe(effects[0])
})
it('rebases Tint amount and independent endpoint component curves without changing source colors',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'color_tint',parameters:{black:[-0.5,0,0,0.25],white:[1.5,1,1,0.75]},animation:curve,colorAnimation:{blackR:curve,whiteA:curve,whiteB:constant(0.8)}}]
 const original=JSON.stringify(effects)
 const shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='color_tint')throw new Error('Tint')
 const source=effects[0]!
 if(source.kind!=='color_tint')throw new Error('source Tint')
 expect(shifted.parameters).toEqual(source.parameters)
 for(const time of [0,250000,500000,1500000]){
  for(const value of [shifted.animation!,shifted.colorAnimation!.blackR!,shifted.colorAnimation!.whiteA!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 }
 expect(shifted.colorAnimation!.blackR).not.toBe(shifted.colorAnimation!.whiteA)
 expect(shifted.colorAnimation!.whiteB).toEqual(constant(0.8))
 expect(JSON.stringify(effects)).toBe(original)
})
it('rebases Leave Color scalar and RGBA key curves independently',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'leave_color',parameters:{color:[1,0,0,1]},animation:{amount:curve,tolerance:constant(15)},colorAnimation:{r:curve,g:curve,a:constant(0.25)}}]
 const original=JSON.stringify(effects)
 const shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='leave_color')throw new Error('Leave Color')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!.amount!,shifted.colorAnimation!.r!,shifted.colorAnimation!.g!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.colorAnimation!.r).not.toBe(shifted.colorAnimation!.g)
 expect(shifted.colorAnimation!.a).toEqual(constant(0.25));expect(shifted.parameters.color).toEqual([1,0,0,1]);expect(JSON.stringify(effects)).toBe(original)
})
it('rebases Change to Color scalar and both endpoint curves preserving authored endpoint alpha',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'change_to_color',parameters:{from:[1,0,0,0],to:[0,0,1,0.25]},animation:{hueTol:curve},colorAnimation:{fromR:curve,toB:curve,toA:constant(0.25)}}]
 const before=JSON.stringify(effects);const shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='change_to_color')throw new Error('Change to Color')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!.hueTol!,shifted.colorAnimation!.fromR!,shifted.colorAnimation!.toB!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.colorAnimation!.fromR).not.toBe(shifted.colorAnimation!.toB);expect(shifted.parameters.to).toEqual([0,0,1,0.25]);expect(JSON.stringify(effects)).toBe(before)
})
it('rebases Color Pass similarity and selected color component curves preserving reverse',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'color_pass',parameters:{reverse:true,color:[1,0,0,0]},animation:curve,colorAnimation:{r:curve,g:curve,a:constant(0)}}]
 const before=JSON.stringify(effects);const shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='color_pass')throw new Error('Color Pass')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!,shifted.colorAnimation!.r!,shifted.colorAnimation!.g!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters.reverse).toBe(true);expect(shifted.colorAnimation!.r).not.toBe(shifted.colorAnimation!.g);expect(JSON.stringify(effects)).toBe(before)
})
it('rebases Color Replace similarity and selected color component curves preserving reverse',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'color_replace',parameters:{solid:true,target:[1,0,0,0],replace:[0,0,1,0]},animation:curve,colorAnimation:{targetR:curve,replaceG:curve,replaceA:constant(0)}}]
 const before=JSON.stringify(effects);const shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='color_replace')throw new Error('Color Replace')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!,shifted.colorAnimation!.targetR!,shifted.colorAnimation!.replaceG!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters.solid).toBe(true);expect(shifted.colorAnimation!.targetR).not.toBe(shifted.colorAnimation!.replaceG);expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Channel Mix coefficients while retaining monochrome and authored constants',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'channel_mix',parameters:{monochrome:true,rc:-25},animation:{rr:curve,rb:curve,gc:constant(-50)}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='channel_mix')throw new Error('Channel Mix')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!.rr!,shifted.animation!.rb!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.animation!.rr).not.toBe(shifted.animation!.rb);expect(shifted.animation!.gc).toEqual(constant(-50));expect(shifted.parameters).toEqual({monochrome:true,rc:-25});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Alpha Adjust opacity without changing any authored flags',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'alpha_adjust',parameters:{ignore:true,invert:true,maskOnly:true},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='alpha_adjust')throw new Error('Alpha Adjust')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({ignore:true,invert:true,maskOnly:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Luma Key threshold and cutoff independently preserving crossed authored values',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'luma_key',parameters:{threshold:25,cutoff:75},animation:{threshold:curve,cutoff:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='luma_key')throw new Error('Luma Key')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!.threshold!,shifted.animation!.cutoff!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.animation!.threshold).not.toBe(shifted.animation!.cutoff);expect(shifted.parameters).toEqual({threshold:25,cutoff:75});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Color Key scalar and RGBA curves retaining authored thin',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'color_key',parameters:{thin:5,color:[0,0,1,0]},animation:{tolerance:curve,feather:curve,thin:constant(5)},colorAnimation:{r:curve,b:curve,a:constant(0)}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='color_key')throw new Error('Color Key')
 for(const time of [0,250000,500000,1500000])for(const value of [shifted.animation!.tolerance!,shifted.animation!.feather!,shifted.colorAnimation!.r!,shifted.colorAnimation!.b!])expect(sampleAnimatableValue(value,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({thin:5,color:[0,0,1,0]});expect(shifted.animation!.thin).toEqual(constant(5));expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Gaussian Blur blurriness preserving axis and repeated edges',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'gaussian_blur',parameters:{dimensions:2,repeatEdge:true},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='gaussian_blur')throw new Error('Gaussian Blur')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({dimensions:2,repeatEdge:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Directional Blur angle and length curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'directional_blur',parameters:{direction:-720,length:1000},animation:{direction:curve,length:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='directional_blur')throw new Error('Directional Blur')
 for(const time of [0,250000,500000,1500000])for(const channel of ['direction','length'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({direction:-720,length:1000});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Camera Blur percent preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'camera_blur',parameters:{percent:100},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='camera_blur')throw new Error('Camera Blur')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({percent:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Median radius preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'median',parameters:{radius:100},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='median')throw new Error('Median')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({radius:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Mosaic blocks and softness curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'mosaic',parameters:{horizontal:2.5,vertical:3.5,sharp:true},animation:{horizontal:curve,vertical:curve,softness:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='mosaic')throw new Error('Mosaic')
 for(const time of [0,250000,500000,1500000])for(const channel of ['horizontal','vertical','softness'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({horizontal:2.5,vertical:3.5,sharp:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Find Edges blend preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'find_edges',parameters:{blend:100,invert:true},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='find_edges')throw new Error('Find Edges')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({blend:100,invert:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Emboss direction relief contrast blend curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'emboss',parameters:{direction:-720,relief:10,contrast:500,blend:100},animation:{direction:curve,relief:curve,contrast:curve,blend:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='emboss')throw new Error('Emboss')
 for(const time of [0,250000,500000,1500000])for(const channel of ['direction','relief','contrast','blend'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({direction:-720,relief:10,contrast:500,blend:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Replicate count preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'replicate',parameters:{count:16},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='replicate')throw new Error('Replicate')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({count:16});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Strobe timing blend and RGBA independently without changing source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'strobe',parameters:{color:[-1,2,0.5,0],blend:100,duration:0,period:0},animation:{colorR:curve,colorG:curve,colorB:curve,colorA:curve,blend:curve,duration:curve,period:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='strobe')throw new Error('Strobe')
 for(const time of [0,250000,500000,1500000])for(const channel of ['colorR','colorG','colorB','colorA','blend','duration','period'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({color:[-1,2,0.5,0],blend:100,duration:0,period:0});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Noise amount and grain size preserving color and clipping',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'noise',parameters:{color:false,clip:false,amount:100,grainSize:2.5},animation:{amount:curve,grainSize:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='noise')throw new Error('Noise')
 for(const time of [0,250000,500000,1500000])for(const channel of ['amount','grainSize'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({color:false,clip:false,amount:100,grainSize:2.5});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Twirl angle radius and center curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'twirl',parameters:{angle:-720,radius:100,center:[1.23456789012345,-2]},animation:{angle:curve,radius:curve,centerX:curve,centerY:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='twirl')throw new Error('Twirl')
 for(const time of [0,250000,500000,1500000])for(const channel of ['angle','radius','centerX','centerY'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({angle:-720,radius:100,center:[1.23456789012345,-2]});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Wave Warp height width direction speed curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'wave_warp',parameters:{height:-10,width:40,direction:-720,speed:-1},animation:{height:curve,width:curve,direction:curve,speed:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='wave_warp')throw new Error('Wave Warp')
 for(const time of [0,250000,500000,1500000])for(const channel of ['height','width','direction','speed'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({height:-10,width:40,direction:-720,speed:-1});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Lens Distortion curvature and decentering curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'lens_distortion',parameters:{curvature:-100,hDecentering:50,vDecentering:-50},animation:{curvature:curve,hDecentering:curve,vDecentering:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='lens_distortion')throw new Error('Lens Distortion')
 for(const time of [0,250000,500000,1500000])for(const channel of ['curvature','hDecentering','vDecentering'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({curvature:-100,hDecentering:50,vDecentering:-50});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases independent Basic 3D swivel tilt distance curves',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'basic_3d',parameters:{swivel:-720,tilt:90,distance:-10000},animation:{swivel:curve,tilt:curve,distance:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='basic_3d')throw new Error('Basic 3D')
 for(const time of [0,250000,500000,1500000])for(const channel of ['swivel','tilt','distance'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({swivel:-720,tilt:90,distance:-10000});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Drop Shadow opacity direction distance softness and RGBA independently without changing source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'drop_shadow',parameters:{color:[-1,2,0.5,0],opacity:100,direction:-720,distance:4000,softness:1000,only:true},animation:{colorR:curve,colorG:curve,colorB:curve,colorA:curve,opacity:curve,direction:curve,distance:curve,softness:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='drop_shadow')throw new Error('Drop Shadow')
 for(const time of [0,250000,500000,1500000])for(const channel of ['colorR','colorG','colorB','colorA','opacity','direction','distance','softness'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({color:[-1,2,0.5,0],opacity:100,direction:-720,distance:4000,softness:1000,only:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Bevel Alpha thickness angle intensity and RGBA independently without changing source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'bevel_alpha',parameters:{color:[-1,2,0.5,0],thickness:200,angle:-720,intensity:100},animation:{colorR:curve,colorG:curve,colorB:curve,colorA:curve,thickness:curve,angle:curve,intensity:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='bevel_alpha')throw new Error('Bevel Alpha')
 for(const time of [0,250000,500000,1500000])for(const channel of ['colorR','colorG','colorB','colorA','thickness','angle','intensity'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({color:[-1,2,0.5,0],thickness:200,angle:-720,intensity:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all eighteen gradient channels independently preserving source colors',()=>{
 const channels=['c1R','c1G','c1B','c1A','c2R','c2G','c2B','c2A','c3R','c3G','c3B','c3A','c4R','c4G','c4B','c4A','blend','opacity'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"four_color_gradient",parameters:{c1:[-1,2,0.5,0],c2:[0,1,0,1],c3:[1,0,1,1],c4:[0,0,1,1],blend:1000,opacity:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="four_color_gradient")throw new Error("gradient")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="four_color_gradient"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all thirteen Ramp channels independently preserving source auto axes and shape',()=>{
 const channels=['startX','startY','endX','endY','startColorR','startColorG','startColorB','startColorA','endColorR','endColorG','endColorB','endColorA','blend'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"ramp",parameters:{start:[null,0],end:[1.23456789012345,null],startColor:[-1,2,0.5,0],endColor:[1,1,1,2],shape:"radial",blend:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="ramp")throw new Error("Ramp")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="ramp"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all eight Circle channels independently preserving source auto axes and color',()=>{
 const channels=['centerX','centerY','radius','colorR','colorG','colorB','colorA','opacity'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"circle",parameters:{center:[1.23456789012345,null],color:[-1,2,0.5,0],radius:4000,opacity:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="circle")throw new Error("Circle")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="circle"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all seven Grid channels independently preserving source grid dimensions and color',()=>{
 const channels=['size','border','colorR','colorG','colorB','colorA','opacity'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"grid",parameters:{size:2.5,border:0.5,color:[-1,2,0.5,0],opacity:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="grid")throw new Error("Grid")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="grid"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all ten Lens Flare channels independently preserving source auto axes and lens type',()=>{
 const channels=['centerX','centerY','brightness','blend','size','ghosts','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"lens_flare",parameters:{center:[1.23456789012345,null],color:[-1,2,0.5,0],brightness:300,blend:25,size:400,ghosts:3.5,lensType:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="lens_flare")throw new Error("Lens Flare")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="lens_flare"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all ten Checkerboard channels independently preserving source auto axes square and blending mode',()=>{
 const channels=['anchorX','anchorY','width','height','feather','opacity','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"checkerboard",parameters:{anchor:[1.23456789012345,null],color:[-1,2,0.5,0],width:2.5,height:4.5,feather:0.5,opacity:25,square:false,mode:5},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="checkerboard")throw new Error("Checkerboard")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="checkerboard"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all seven Cell Pattern channels independently preserving source auto axes pattern overflow and invert',()=>{
 const channels=['offsetX','offsetY','contrast','disperse','size','evolution','seed'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"cell_pattern",parameters:{offset:[1.23456789012345,null],pattern:7,overflow:2,invert:true,contrast:123,disperse:1.5,size:2.5,evolution:-90,seed:2.5},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="cell_pattern")throw new Error("Cell Pattern")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="cell_pattern"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all fourteen Ellipse channels independently preserving source auto axes colors and composite',()=>{
 const channels=['centerX','centerY','width','height','thickness','softness','insideR','insideG','insideB','insideA','outsideR','outsideG','outsideB','outsideA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"ellipse",parameters:{center:[1.23456789012345,null],width:5.5,height:8.5,thickness:2.5,softness:15,inside:[-1,2,.5,0],outside:[1,0,2,0],composite:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="ellipse")throw new Error("Ellipse")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="ellipse"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all eight Paint Bucket channels independently preserving source auto axes selector mode and invert',()=>{
 const channels=['pointX','pointY','tolerance','opacity','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"paint_bucket",parameters:{point:[1.23456789012345,null],tolerance:10,opacity:25,color:[-1,2,.5,0],selector:3,mode:4,invert:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="paint_bucket")throw new Error("Paint Bucket")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="paint_bucket"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all fifteen Gradient channels independently preserving source auto axes endpoint colors shape scatter midpoint and blend',()=>{
 const channels=['startX','startY','endX','endY','startColorR','startColorG','startColorB','startColorA','endColorR','endColorG','endColorB','endColorA','scatter','midpoint','blend'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"gradient",parameters:{start:[1.23456789012345,null],end:[null,2.125],startColor:[-1,2,.5,0],endColor:[1,0,2,.5],shape:3,scatter:100,midpoint:50,blend:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="gradient")throw new Error("Gradient")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="gradient"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})


it('rebases four distinct Color Emboss curves preserving source parameters and local sampling',()=>{
 const channels=['direction','relief','contrast','blend'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"color_emboss",parameters:{direction:720,relief:1,contrast:123,blend:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="color_emboss")throw new Error("Color Emboss")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="color_emboss"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases six distinct Stroke curves preserving source parameters and local sampling',()=>{
 const channels=['width','opacity','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"stroke",parameters:{width:4,opacity:25,color:[-1,2,.5,0],position:2,only:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="stroke")throw new Error("Stroke")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="stroke"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases fourteen distinct Roughen Edges curves preserving source parameters and local sampling',()=>{
 const channels=['border','sharpness','influence','scale','stretch','complexity','evolution','seed','offsetX','offsetY','edgeColorR','edgeColorG','edgeColorB','edgeColorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"roughen_edges",parameters:{border:8,edgeColor:[-1,2,.5,0],offset:[1.23456789012345,-2.125],edgeType:7},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="roughen_edges")throw new Error("Stroke")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="roughen_edges"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases six distinct Brush Strokes curves preserving source parameters and local sampling',()=>{
 const channels=['angle','size','length','density','randomness','blend'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"brush_strokes",parameters:{angle:720,size:2,length:10,density:1,randomness:1,blend:25,surface:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="brush_strokes")throw new Error("Brush Strokes")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="brush_strokes"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases seven distinct Long Shadow curves preserving source parameters and local sampling',()=>{
 const channels=['angle','length','opacity','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"long_shadow",parameters:{angle:135,length:100,opacity:25,color:[-1,2,.5,0],fade:false,only:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="long_shadow")throw new Error("Stroke")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="long_shadow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases ten distinct Alpha Glow curves preserving source parameters and local sampling',()=>{
 const channels=['glow', 'brightness', 'startColorR', 'startColorG', 'startColorB', 'startColorA', 'endColorR', 'endColorG', 'endColorB', 'endColorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"alpha_glow",parameters:{glow:30,brightness:252,startColor:[-1,2,.5,0],endColor:[1,0,2,.5],useEnd:true,fadeOut:false},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="alpha_glow")throw new Error("Stroke")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="alpha_glow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct Block Dissolve curves preserving source parameters and local sampling',()=>{
 const channels=['completion','blockW','blockH','feather'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"block_dissolve",parameters:{completion:50,blockW:2.5,blockH:3.5,feather:25,soft:false},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="block_dissolve")throw new Error("Block Dissolve")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="block_dissolve"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases two distinct Gradient Wipe curves preserving source parameters and local sampling',()=>{
 const channels=['completion','softness'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"gradient_wipe",parameters:{completion:50,softness:25,layer:2,placement:1,invert:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="gradient_wipe")throw new Error("Gradient Wipe")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="gradient_wipe"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three distinct Linear Wipe curves preserving source parameters and local sampling',()=>{
 const channels=['completion','angle','feather'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"linear_wipe",parameters:{completion:50,angle:720,feather:25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="linear_wipe")throw new Error("Linear Wipe")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="linear_wipe"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases six distinct Magnify curves preserving source parameters and local sampling',()=>{
 const channels=['centerX','centerY','magnification','size','feather','opacity'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"magnify",parameters:{shape:1,mode:4,center:[1.23456789012345,null],magnification:200,size:2.5,feather:25,opacity:50},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="magnify")throw new Error("Magnify")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="magnify"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three distinct Spherize curves preserving source parameters and local sampling',()=>{
 const channels=['radius','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"spherize",parameters:{radius:100,center:[1.23456789012345,null]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="spherize")throw new Error("Spherize")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="spherize"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three distinct Mirror curves preserving source parameters and local sampling',()=>{
 const channels=['angle','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"mirror",parameters:{angle:100,center:[1.23456789012345,null]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="mirror")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="mirror"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three distinct Offset curves preserving source parameters and local sampling',()=>{
 const channels=['blend','shiftX','shiftY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"offset",parameters:{blend:100,shift:[1.23456789012345,null]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="offset")throw new Error("Offset")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="offset"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases eight distinct Corner Pin curves preserving source parameters and local sampling',()=>{
 const channels=['upperLeftX','upperLeftY','upperRightX','upperRightY','lowerLeftX','lowerLeftY','lowerRightX','lowerRightY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"corner_pin",parameters:{upperLeft:[1.23456789012345,null]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="corner_pin")throw new Error("CornerPin")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="corner_pin"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases five distinct 3D Rotate curves preserving source parameters and local sampling',()=>{
 const channels=['rotX','rotY','rotZ','perspective','z'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"rotate_3d",parameters:{rotX:30,rotY:45,rotZ:-15,perspective:50,z:0,hideBack:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="rotate_3d")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="rotate_3d"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct grow curves preserving source parameters and local sampling',()=>{
 const channels=['from','to','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"grow",parameters:{from:100,to:120,center:[1.123456789012,null],easing:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="grow")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="grow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct shrink curves preserving source parameters and local sampling',()=>{
 const channels=['from','to','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"shrink",parameters:{from:100,to:120,center:[1.123456789012,null],easing:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="shrink")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="shrink"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct Spin curves preserving source parameters and local sampling',()=>{
 const channels=['amount','scale','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"spin",parameters:{amount:360,scale:100,center:[1.123456789012,null],easing:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="spin")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="spin"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases five distinct Move curves preserving source parameters and local sampling',()=>{
 const channels=['fromX','fromY','toX','toY','motionBlur'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"move",parameters:{from:[-2.123456789012,1.25],to:[3.5,-0.75],motionBlur:50,easing:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="move")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="move"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases five distinct Wiggle curves preserving source parameters and local sampling',()=>{
 const channels=['frequency','amount','rotation','scale','seed'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"wiggle",parameters:{frequency:3.25,amount:2.5,rotation:37,scale:100,dimensions:2,seed:42.75},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="wiggle")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="wiggle"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases seven distinct CameraShake curves preserving source parameters and local sampling',()=>{
 const channels=['frequency','amount','rotation','zoom','complexity','motionBlur','seed'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"camera_shake",parameters:{frequency:4,amount:2.5,rotation:3.75,zoom:25,complexity:3.75,motionBlur:50,seed:42.75},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="camera_shake")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="camera_shake"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases nine distinct Spacer curves preserving source parameters and local sampling',()=>{
 const channels=['left','top','right','bottom','radius','colorR','colorG','colorB','colorA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"spacer",parameters:{left:0.75,top:1.25,right:2.5,bottom:0.5,uniform:false,radius:2,fill:true,color:[0.25,0.5,1,0]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="spacer")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="spacer"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases eleven distinct RoundedCrop curves preserving source parameters and local sampling',()=>{
 const channels=['left','top','right','bottom','radius','feather','border','borderR','borderG','borderB','borderA'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"rounded_crop",parameters:{left:10,top:20,right:30,bottom:5,radius:0.75,feather:0.75,border:0.75,borderColor:[0.25,0.5,1,0]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="rounded_crop")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="rounded_crop"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three distinct Clone curves preserving source parameters and local sampling',()=>{
 const channels=['columns','rows','gap'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"clone",parameters:{columns:2.75,rows:1.75,gap:0.75,mirror:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="clone")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="clone"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases two distinct AutoAlign curves preserving source parameters and local sampling',()=>{
 const channels=['marginX','marginY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"auto_align",parameters:{horizontal:1,vertical:3,marginX:0.75,marginY:2.25},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="auto_align")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="auto_align"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases eight distinct TurbulentDisplace curves preserving source parameters and local sampling',()=>{
 const channels=['amount','size','offsetX','offsetY','complexity','evolution','cycleRevs','seed'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"turbulent_displace",parameters:{displacement:5,amount:2.5,size:3.25,offset:[3.123456789012,null],complexity:3.75,evolution:-450,cycle:true,cycleRevs:2.25,seed:42.75,pinning:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="turbulent_displace")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="turbulent_displace"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct ChannelBlur curves preserving source parameters and local sampling',()=>{
 const channels=['red','green','blue','alpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"channel_blur",parameters:{red:2.5,green:0,blue:4,alpha:1,dimensions:2,repeatEdge:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="channel_blur")throw new Error("Rotate3D")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="channel_blur"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases six distinct FocusBlur curves preserving source parameters and local sampling',()=>{
 const channels=['amount','size','feather','angle','centerX','centerY'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"focus_blur",parameters:{amount:4,size:3.25,feather:1.5,angle:-37,center:[3.123456789012,null],shape:1,show:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="focus_blur")throw new Error("FocusBlur")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="focus_blur"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases four distinct BokehBlur curves preserving source parameters and local sampling',()=>{
 const channels=['amount','rotation','highlight','threshold'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"bokeh_blur",parameters:{amount:13.25,rotation:-37,highlight:50,threshold:80,shape:5},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="bokeh_blur")throw new Error("BokehBlur")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="bokeh_blur"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Compound Blur max preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'compound_blur',parameters:{max:4.25,layer:2,stretch:false,invert:true},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='compound_blur')throw new Error('Compound Blur')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({max:4.25,layer:2,stretch:false,invert:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Sharpen amount preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'sharpen',parameters:{amount:4000},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='sharpen')throw new Error('Sharpen')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({amount:4000});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases three Unsharp Mask curves preserving authored source parameters',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'unsharp_mask',parameters:{amount:500,radius:250,threshold:255},animation:{amount:curve,radius:curve,threshold:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='unsharp_mask')throw new Error('Unsharp Mask')
 for(const time of [0,250000,500000,1500000])for(const channel of ['amount','radius','threshold'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({amount:500,radius:250,threshold:255});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases two Brightness & Contrast curves preserving authored source parameters',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'brightness_contrast',parameters:{brightness:-100,contrast:100},animation:{brightness:curve,contrast:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='brightness_contrast')throw new Error('Brightness & Contrast')
 for(const time of [0,250000,500000,1500000])for(const channel of ['brightness','contrast'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({brightness:-100,contrast:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases five Crop curves preserving source parameters and Zoom',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'crop',parameters:{left:12.5,top:20,right:3.25,bottom:7.5,feather:4.25,zoom:true},animation:{left:curve,top:curve,right:curve,bottom:curve,feather:curve}}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='crop')throw new Error('Crop')
 for(const time of [0,250000,500000,1500000])for(const channel of ['left','top','right','bottom','feather'] as const)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({left:12.5,top:20,right:3.25,bottom:7.5,feather:4.25,zoom:true});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Edge Feather amount preserving authored source controls',()=>{
 const effects:readonly WireVideoEffect[]=[{kind:'edge_feather',parameters:{amount:100},animation:curve}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='edge_feather')throw new Error('Edge Feather')
 for(const time of [0,250000,500000,1500000])expect(sampleAnimatableValue(shifted.animation!,time)).toBe(sampleAnimatableValue(curve,time+500000))
 expect(shifted.parameters).toEqual({amount:100});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases ten ASC CDL curves preserving all source parameters and local samples',()=>{
 const channels=['rSlope','rOffset','rPower','gSlope','gOffset','gPower','bSlope','bOffset','bPower','saturation'] as const
 if(curve.mode!=='keyframes')throw new Error('curve')
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:'asc_cdl',parameters:{rSlope:0.75,rOffset:-0.125,gPower:1.25,saturation:0.625},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='asc_cdl')throw new Error('ASC CDL')
 for(const time of[0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==='asc_cdl'?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases eleven Transform curves preserving precise points and uniform-scale control',()=>{
 const channels=['scaleHeight','scaleWidth','skew','skewAxis','rotation','opacity','shutterAngle','anchorX','anchorY','positionX','positionY'] as const
 if(curve.mode!=='keyframes')throw new Error('curve')
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:'transform',parameters:{anchor:[1.123456789012,null],position:[null,-0.75],uniformScale:false,scaleHeight:75,scaleWidth:125,shutterAngle:180},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='transform')throw new Error('Transform')
 for(const time of[0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==='transform'?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases eight standalone Vignette curves preserving color alpha and source parameters',()=>{
 const channels=['amount','midpoint','roundness','feather','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=='keyframes')throw new Error('curve')
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:'vignette',parameters:{amount:-50,roundness:-25,color:[0.125,0.75,0.25,0]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='vignette')throw new Error('Vignette')
 for(const time of[0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==='vignette'?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases all64 Lighting curves preserving five lights and precise automatic axes',async()=>{
 const {lightingChannels}=await import('./lightingEffects');const channels=lightingChannels.map(c=>c[0]);
 if(curve.mode!=='keyframes')throw new Error('curve');
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]));
 const parameters={lights:Array.from({length:5},(_,i)=>({kind:i===0?3 as const:0 as const,center:[1.123456789012,null] as const,color:[0.125,0.75,0.25,0] as const})),whiteHigh:false,ambient:-50};
 const effects:readonly WireVideoEffect[]=[{kind:'lighting_effects',parameters,animation}];const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!;if(shifted.kind!=='lighting_effects')throw new Error('Lighting');
 for(const time of[0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000));expect(shifted.parameters).toEqual(parameters);expect(JSON.stringify(effects)).toBe(before);
})

it('rebases six Logo Cutout curves preserving color alpha and source parameters',()=>{
 const channels=['threshold','softness','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=='keyframes')throw new Error('curve')
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:'logo_cutout',parameters:{background:2,unmultiply:false,invert:true,threshold:5,softness:10,color:[0.125,0.75,0.25,0]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=='logo_cutout')throw new Error('Vignette')
 for(const time of[0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==='logo_cutout'?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})


it('rebases five distinct RGB Split curves preserving source parameters and local sampling',()=>{
 const channels=['amount','angle','centerX','centerY','blend'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"rgb_split",parameters:{mode:1,amount:10,blend:37.5,angle:100,center:[1.23456789012345,null]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="rgb_split")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="rgb_split"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})


it('rebases eight distinct Edge Glow curves preserving source parameters and local sampling',()=>{
 const channels=['threshold','width','radius','intensity','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"edge_glow",parameters:{threshold:20,width:2,radius:10,intensity:150,color:[0.3,0.8,1,0],only:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="edge_glow")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="edge_glow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})


it('rebases ten distinct Echo Glow curves preserving source parameters and local sampling',()=>{
 const channels=['threshold','echoes','radius','spread','intensity','decay','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"echo_glow",parameters:{threshold:60,echoes:3.75,radius:12,spread:1.8,intensity:100,decay:50,color:[1,0.85,0.6,0]},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="echo_glow")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="echo_glow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})


it('rebases eight distinct Wonder Glow curves preserving source parameters and local sampling',()=>{
 const channels=['threshold','saturation','radius','intensity','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"wonder_glow",parameters:{threshold:60,saturation:100,radius:25,intensity:100,color:[1,0.8,0.5,0],mode:1,useColor:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="wonder_glow")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="wonder_glow"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})



it('rebases nine distinct Glint curves preserving source parameters and local sampling',()=>{
 const channels=['threshold','rotation','colorize','length','intensity','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"glint",parameters:{threshold:75,rotation:45,colorize:0,length:60,intensity:100,color:[1,1,1,0],rays:3},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="glint")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="glint"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases nine distinct Volumetric Rays curves preserving source parameters and local sampling',()=>{
 const channels=['threshold','centerX','centerY','length','intensity','colorRed','colorGreen','colorBlue','colorAlpha'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"volumetric_rays",parameters:{threshold:60,center:[1.123456789012,null],length:50,intensity:100,color:[1,1,1,0],only:true},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="volumetric_rays")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="volumetric_rays"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('rebases Posterize Time rate curves preserving source parameters and local sampling',()=>{
 const channels=['rate'] as const
 if(curve.mode!=="keyframes")throw new Error("curve")
 const animation=Object.fromEntries(channels.map((name,index)=>[name,{...curve,track:{...curve.track,keyframes:curve.track.keyframes.map(key=>({...key,value:key.value+index}))}}]))
 const effects:readonly WireVideoEffect[]=[{kind:"posterize_time",parameters:{rate:23.976},animation}]
 const before=JSON.stringify(effects),shifted=rebaseNativeEffects(effects,-500000)![0]!
 if(shifted.kind!=="posterize_time")throw new Error("Mirror")
 for(const time of [0,250000,500000,1500000])for(const channel of channels)expect(sampleAnimatableValue(shifted.animation![channel]!,time)).toBe(sampleAnimatableValue(animation[channel]!,time+500000))
 expect(shifted.parameters).toEqual(effects[0]!.kind==="posterize_time"?effects[0]!.parameters:{});expect(JSON.stringify(effects)).toBe(before)
})

it('preserves Auto Reframe subject coordinates and temporal Echo curves across signed local rebasing',()=>{
 const make=(a:number,b:number):WireAnimatableValue=>({mode:'keyframes',track:{timeBase:1000,interpolation:'linear',extrapolation:{before:'hold',after:'hold'},keyframes:[{tick:-250,value:a},{tick:1250,value:b}]}})
 const effects:readonly WireVideoEffect[]=[
  {kind:'auto_reframe',parameters:{preset:2,aspect:4,zoom:175,offset:[1.123456789012,-.75]},animation:{zoom:make(100,400),offsetX:make(1.123456789012,2.123456789012),offsetY:make(-.75,.25)}},
  {kind:'echo',parameters:{time:-.033,count:3.75,start:.5,decay:1.25,operator:6},animation:{time:make(-.1,.1),count:make(1,6.5),start:make(1,.25),decay:make(.5,1.5)}},
 ]
 const before=JSON.stringify(effects)
 const shifted=rebaseNativeEffects(effects,-500000)!
 for(let i=0;i<effects.length;i++){
  const original=effects[i]!,result=shifted[i]!
  if((original.kind!=='auto_reframe'&&original.kind!=='echo')||(result.kind!=='auto_reframe'&&result.kind!=='echo'))throw new Error('temporal effects')
  expect(result.parameters).toEqual(original.parameters)
  expect(result.parameters).not.toBe(original.parameters)
  for(const name of Object.keys(original.animation!)){
   const source=(original.animation as Record<string,WireAnimatableValue>)[name]!
   const target=(result.animation as Record<string,WireAnimatableValue>)[name]!
   expect(target).toMatchObject({track:{timeBase:1000,keyframes:[{tick:-750},{tick:750}]}})
   for(const tick of [-1000000,-750000,-250000,0,500000,750000,1500000])expect(sampleAnimatableValue(target,tick)).toBeCloseTo(sampleAnimatableValue(source,tick+500000),12)
   expect(target).not.toBe(source)
  }
 }
 expect(rebaseNativeEffects(shifted,500000)).toEqual(effects)
 expect(JSON.stringify(effects)).toBe(before)
})
