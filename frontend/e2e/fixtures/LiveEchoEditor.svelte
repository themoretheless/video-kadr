<script lang="ts">
import {renderNestedPreview} from '../../src/lib/api'
import Inspector from '../../src/lib/components/composition/NestedEchoInspector.svelte'
import {nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '../../src/lib/state/nestedComposition.svelte'
let {token}:{token:string}=$props()
let canvas:HTMLCanvasElement
let error=$state('')
async function preview() {
 try {
  const frame=await renderNestedPreview(nestedComposition.document!,nestedComposition.timeUs,{},token,undefined,'root')
  canvas.width=frame.width;canvas.height=frame.height
  const context=canvas.getContext('2d')!
  const pixels=context.createImageData(frame.width,frame.height);pixels.data.set(frame.rgba);context.putImageData(pixels,0,0)
 }catch(failure){error=String(failure)}
}
</script>
<Inspector />
<button onclick={undoNestedComposition}>Undo</button><button onclick={redoNestedComposition}>Redo</button>
<output aria-label="Echo history">{nestedHistory.past.length}</output>
<output aria-label="Echo error">{nestedHistory.error ?? ''}</output>
<output aria-label="Echo document" style="display:block;overflow-wrap:anywhere;max-height:140px;overflow:auto">{JSON.stringify(nestedComposition.document)}</output>

<button onclick={preview}>Native Preview</button><output aria-label="Echo preview error">{error}</output><canvas aria-label="Echo preview" bind:this={canvas} style="display:block;image-rendering:pixelated"></canvas>
