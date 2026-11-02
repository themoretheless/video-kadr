import {expect,it} from 'vitest'
import {graphRange,graphPoint,clientGraphPoint,replaceGraphPoint,sameGraphPoints} from './sourceBezierGraph'
it('maps CSS-pixel coordinates including SVG letterboxing without clipping finite overshoot',()=>{
 const points=[0.42,-2,0.58,3] as const,range=graphRange(points)
 const [x,y]=graphPoint(points[0],points[1],range)
 const rect={left:40,top:60,width:560,height:500}
 // ViewBox280x180 at scale2, vertically centered with70px bars.
 const value=clientGraphPoint(rect.left+x*2,rect.top+70+y*2,rect,range)!
 expect(value[0]).toBeCloseTo(0.42,12);expect(value[1]).toBeCloseTo(-2,12)
 expect(replaceGraphPoint(points,0,-1,-4)).toEqual([0,-4,0.58,3])
 expect(clientGraphPoint(0,0,{left:0,top:0,width:0,height:0},range)).toBeNull()
})
it('keeps source points immutable and graph projection finite at large finite y values',()=>{
 const points=[0.42,1e308,0.58,-1e308] as const
 const range=graphRange(points);expect(graphPoint(points[0],points[1],range).every(Number.isFinite)).toBe(true)
 const changed=replaceGraphPoint(points,1,2,20);expect(changed).toEqual([0.42,1e308,1,20]);expect(points[3]).toBe(-1e308)
 expect(sameGraphPoints(points,points)).toBe(true)
 expect(()=>replaceGraphPoint(points,0,0,Infinity)).toThrow()
})
