<script lang="ts">
  import Controls from '../NestedPlaybackControls.svelte'
  import Preview from '../NestedCompositionPreview.svelte'
  import { nestedComposition, nestedPlayback } from '$lib/state/nestedComposition.svelte'
  let { token }: { token: string } = $props()
</script>
<Controls {token} />
{#if nestedComposition.document}
  <Preview project={nestedComposition.document} compositionId={nestedComposition.activeCompositionId ?? nestedComposition.document.root} timeUs={nestedComposition.timeUs} renderedOutputs={nestedComposition.renderedOutputs} playing={nestedPlayback.playing} playbackEpoch={nestedPlayback.epoch} {token} />
{/if}
