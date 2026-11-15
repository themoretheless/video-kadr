<script lang="ts">
 import type { readConcatTemplateFolder } from '$lib/composition/concatTemplateFolder'

 let { name, format, slotCount, slots, poster }: { name: string } &
  Pick<Awaited<ReturnType<typeof readConcatTemplateFolder>>, 'format' | 'slotCount' | 'slots' | 'poster'> = $props()
 const kinds = new Map([['video', 'Видео'], ['audio', 'Аудио'], ['image', 'Изображение']])
 const seconds = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 3 })
 let posterUrl = $state('')

 $effect(() => {
  if (!poster) {
   posterUrl = ''
   return
  }
  const url = URL.createObjectURL(new Blob([poster], { type: 'image/jpeg' }))
  posterUrl = url
  return () => URL.revokeObjectURL(url)
 })
</script>

{#if posterUrl}
 <img src={posterUrl} alt="Обложка шаблона Concat" width="160" height="90" loading="lazy" onerror={() => { posterUrl = '' }} />
{/if}
<strong>{name}</strong>
<span>
 {#if format}
  {@const { width, height, rateNum, rateDen } = format}
  {width} × {height} · {(rateNum / rateDen).toLocaleString('ru-RU', { maximumFractionDigits: 3 })} кадр/с ·
 {/if}
 Слоты: {slotCount}
</span>

{#if slots.length}
 <span class="slots" aria-label="Слоты шаблона">
  {#each slots as slot (slot.id)}
   <span>{slot.name} · {kinds.get(slot.kind) ?? slot.kind} · {seconds.format(slot.seconds)} с</span>
  {/each}
 </span>
{/if}

<style>
 img{object-fit:contain;max-width:100%}
 .slots{display:grid;gap:.15rem;overflow-wrap:anywhere}
</style>
