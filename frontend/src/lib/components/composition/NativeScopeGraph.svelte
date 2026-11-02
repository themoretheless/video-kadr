<script lang="ts">
 import type {NativeScopeGraph} from '$lib/api'
 let {graph,height=220}:{graph?:NativeScopeGraph;height?:number}=$props()
 let canvas:HTMLCanvasElement
 $effect(()=>{
  const target=canvas,value=graph
  if(!target)return
  if(!value){target.width=target.width;return}
  const context=target.getContext('2d');if(!context)return
  target.width=value.image.width;target.height=value.image.height
  const image=context.createImageData(target.width,target.height);image.data.set(value.image.rgba);context.putImageData(image,0,0)
 })
</script>
 <div class="plot"><div class="axis left">{#each graph?.axis ?? [] as label}<span style:top={`${label.position}%`}>{label.left}</span>{/each}</div>
 <div class="graph" >
 {#if graph?.axisUnit==='cd/m²'}<span class="axis-unit">cd/m²</span>{/if}<canvas style:height={`${height}px`} class:levels={!!graph?.axis} bind:this={canvas} aria-label="Lumetri Scopes график"></canvas>
 {#if graph?.overlay}<svg aria-hidden="true" preserveAspectRatio={graph!.axis ? 'none' : 'xMidYMid meet'} viewBox={`0 0 ${graph!.image.width} ${graph!.image.height}`}>{#each graph!.overlay ?? [] as mark}
 {#if mark.kind==='line'}<line vector-effect="non-scaling-stroke" x1={mark.x1} y1={mark.y1} x2={mark.x2} y2={mark.y2} stroke={mark.color} />
 {:else if mark.kind==='circle'}<circle vector-effect="non-scaling-stroke" cx={mark.cx} cy={mark.cy} r={mark.r} stroke={mark.color} fill="none" />
 {:else if mark.kind==='rect'}<rect vector-effect="non-scaling-stroke" x={mark.x} y={mark.y} width={mark.width} height={mark.height} stroke={mark.color} fill="none" />
 {:else}<text x={mark.x} y={mark.y} fill={mark.color} text-anchor="middle" dominant-baseline="middle" font-size="10">{mark.text}</text>{/if}
 {/each}</svg>{/if}
 {#if graph?.foreground?.length}<svg class="foreground" aria-hidden="true" preserveAspectRatio="none" viewBox={`0 0 ${graph.image.width} ${graph.image.height}`}>{#each graph.foreground as mark}<line vector-effect="non-scaling-stroke" x1={mark.x1} y1={mark.y1} x2={mark.x2} y2={mark.y2} stroke={mark.color} />{/each}</svg>{/if}
 </div>
 <div class="axis right">{#each graph?.axis ?? [] as label}<span style:top={`${label.position}%`}>{label.right}</span>{/each}</div></div>
<style>
 .plot{display:grid;grid-template-columns:30px minmax(0,1fr) 30px;padding:6px 0}.axis{position:relative;font-size:10px;color:#aaa}.axis span{position:absolute;transform:translateY(-50%)}.left span{right:4px}.right span{left:4px}
 .axis-unit{position:absolute;top:-12px;right:0;font-size:9px;color:#aaa}
 .graph{position:relative;background:#000}.graph svg{position:absolute;inset:0;width:100%;height:100%;pointer-events:none;z-index:0}.graph svg.foreground{z-index:2}canvas{position:relative;z-index:1;display:block;background:transparent;width:100%;height:220px;object-fit:contain}canvas.levels{object-fit:fill}</style>