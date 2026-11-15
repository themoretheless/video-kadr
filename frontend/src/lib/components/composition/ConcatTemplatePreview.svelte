<script lang="ts">
 import type { readConcatTemplateFolder } from '$lib/composition/concatTemplateFolder'

 let { name, format, slotCount, poster }: { name: string } &
  Pick<Awaited<ReturnType<typeof readConcatTemplateFolder>>, 'format' | 'slotCount' | 'poster'> = $props()
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

<style>img{object-fit:contain;max-width:100%}</style>
