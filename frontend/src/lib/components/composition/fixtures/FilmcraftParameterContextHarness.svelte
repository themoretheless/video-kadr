<script lang="ts">
  import type { Writable } from 'svelte/store'
  import type { ComponentProps } from 'svelte'
  import Editor from '../FilmcraftScalarParameterEditor.svelte'
  let { context }: { context: Writable<ComponentProps<typeof Editor>> } = $props()
</script>
<Editor {...$context} />
