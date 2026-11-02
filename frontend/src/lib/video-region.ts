import type {SamVideoRegionAttachment} from './composition/types'
export function validateVideoRegion(value:unknown):asserts value is SamVideoRegionAttachment {
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Invalid video region')
 const r=value as Record<string,unknown>,id=(v:unknown)=>typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(v)&&!['__proto__','prototype','constructor'].includes(v)
 if(!id(r.sourceId)||!id(r.maskSourceId)||r.sourceId===r.maskSourceId)throw new Error('Invalid video region sources')
 for(const key of ['sourceHash','maskHash','maskFileHash','encoderHash','decoderHash','runtimeHash','promptHash'])if(typeof r[key]!=='string'||!/^[a-f0-9]{64}$/.test(r[key] as string))throw new Error('Invalid video region digest')
 if(!Array.isArray(r.points)||!r.points.length||r.points.length>65536||r.points.some(p=>!Array.isArray(p)||p.length!==2||p.some(v=>typeof v!=='number'||!Number.isFinite(v)||v<-.5||v>1.5)))throw new Error('Invalid video region prompts')
 for(const key of ['width','height','frames','seedFrame','trackedFrames'])if(typeof r[key]!=='number'||!Number.isSafeInteger(r[key])||(r[key] as number)<0)throw new Error('Invalid video region dimensions')
 const v=r as unknown as SamVideoRegionAttachment
 if(!v.width||!v.height||!v.frames||v.width*v.height>64*1024*1024||v.frames>360000||v.seedFrame>=v.frames||v.width*v.height*4*v.frames>64*1024*1024*1024)throw new Error('Video region budget')
 if(!v.frameRate||![v.frameRate.numerator,v.frameRate.denominator].every(n=>Number.isInteger(n)&&n>0&&n<=4294967295)||Object.keys(v.frameRate).some(k=>!['numerator','denominator'].includes(k)))throw new Error('Invalid video region clock')
 if(v.forwardLostAt!==null&&(!Number.isInteger(v.forwardLostAt)||v.forwardLostAt<=v.seedFrame||v.forwardLostAt>=v.frames)||v.backwardLostAt!==null&&(!Number.isInteger(v.backwardLostAt)||v.backwardLostAt<0||v.backwardLostAt>=v.seedFrame))throw new Error('Invalid video region losses')
 const forward=(v.forwardLostAt??v.frames)-v.seedFrame-1,backward=v.backwardLostAt===null?v.seedFrame:v.seedFrame-v.backwardLostAt-1
 if(v.trackedFrames!==1+forward+backward)throw new Error('Invalid video region retained count')
 validateVideoFrameTimes(v.frameTimesUs,v.frames)
 if(Object.keys(v).some(k=>!['sourceId','maskSourceId','sourceHash','maskHash','maskFileHash','encoderHash','decoderHash','runtimeHash','promptHash','points','width','height','frames','frameRate','frameTimesUs','seedFrame','trackedFrames','forwardLostAt','backwardLostAt'].includes(k)))throw new Error('Unknown video region field')
}

export function validateVideoFrameTimes(times:unknown,frames:number):asserts times is readonly number[]|undefined {
 if(times!==undefined&&(!Array.isArray(times)||times.length!==frames+1||times[0]!==0||times.some((t,i)=>!Number.isSafeInteger(t)||t<0||(i>0&&t<=times[i-1]))))throw new Error('Invalid video presentation clock')
}
export function videoRegionOrdinal(clock:{frames:number;frameRate:{numerator:number;denominator:number};frameTimesUs?:readonly number[]},sourceUs:number):number {
 if(!Number.isSafeInteger(sourceUs)||sourceUs<0)throw new Error('Invalid video source instant')
 validateVideoFrameTimes(clock.frameTimesUs,clock.frames)
 if(clock.frameTimesUs){const times=clock.frameTimesUs;if(sourceUs>=times[times.length-1]!)throw new Error('Video source instant outside clock');let low=0,high=times.length;while(low<high){const mid=(low+high)>>>1;if(times[mid]!<=sourceUs)low=mid+1;else high=mid}return low-1}
 const ordinal=Number(BigInt(sourceUs)*BigInt(clock.frameRate.numerator)/(1_000_000n*BigInt(clock.frameRate.denominator)))
 if(ordinal>=clock.frames)throw new Error('Video source instant outside clock')
 return ordinal
}
