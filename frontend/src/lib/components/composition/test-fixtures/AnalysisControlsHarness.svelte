<script lang="ts">
 import Controls from '../FilmcraftAnalysisControls.svelte'
 let { onready }: { onready: () => void } = $props()
 let document = $state('first'), token = $state<string | null>('owner')
</script>
<button onclick={() => document = 'second'}>Сменить проект</button>
<button onclick={() => token = 'other'}>Сменить пользователя</button>
<Controls input={{document,sequenceId:'6',timeFrames:'9007199254740993',sources:{'1':'video'}}} {token} {onready} />
