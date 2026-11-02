<script lang="ts">
  import type { Writable } from 'svelte/store'
  import type { ComponentProps } from 'svelte'
  import Controls from '../FilmcraftMaskTrackingControls.svelte'
  let { context }: { context: Writable<ComponentProps<typeof Controls>> } = $props()
</script>
<Controls {...$context} />
