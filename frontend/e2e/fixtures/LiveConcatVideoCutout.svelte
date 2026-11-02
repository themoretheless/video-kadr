<script lang="ts">
 import Panel from '../../src/lib/components/composition/NestedConcatVideoCutout.svelte'
 import {nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '../../src/lib/state/nestedComposition.svelte'
 import {saveCompositionProject} from '../../src/lib/state/composition/projects.svelte'
 import {compositionState} from '../../src/lib/state/composition/core.svelte'
 import {authState} from '../../src/lib/state/auth.svelte'
 import {renderNestedPreview} from '../../src/lib/api'
 let {token}:{token:string}=$props()
 let canvas:HTMLCanvasElement,previewError=$state('')
 async function save(){authState.token=token;await saveCompositionProject()}
 async function preview(){try{previewError='';const frame=await renderNestedPreview(nestedComposition.document!,0,{},token,undefined,'root',{projectId:nestedComposition.projectId!,expectedRevision:nestedComposition.revision!});canvas.width=frame.width;canvas.height=frame.height;const context=canvas.getContext('2d')!;const pixels=context.createImageData(frame.width,frame.height);pixels.data.set(frame.rgba);context.putImageData(pixels,0,0)}catch(error){previewError=String(error)}}
</script>
<Panel {token} compositionId="root" clipId="clip"/>
<button onclick={undoNestedComposition}>Undo</button><button onclick={redoNestedComposition}>Redo</button><button onclick={save} disabled={compositionState.save.busy||nestedHistory.busy}>Save</button><output aria-label="Video cutout save error">{compositionState.save.error}</output>
<output aria-label="Video cutout history">{nestedHistory.past.length}</output>
<output aria-label="Video cutout revision">{nestedComposition.revision}</output>
<output aria-label="Video cutout error">{nestedHistory.error}</output>
<output aria-label="Video cutout document" style="display:block;overflow-wrap:anywhere;max-height:100px;overflow:auto">{JSON.stringify(nestedComposition.document)}</output>
<button onclick={preview}>Native Preview</button><output aria-label="Video cutout preview error">{previewError}</output><canvas bind:this={canvas} aria-label="Video cutout common preview"></canvas>
