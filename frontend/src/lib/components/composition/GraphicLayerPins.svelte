<script lang="ts">
  import { captureGraphicPin } from '$lib/api'
  import type { GraphicLayer, GraphicPin } from '$lib/composition/types'
  let { layer, layers, width = 1920, height = 1080, localUs = 0, onchange }: { layer: GraphicLayer; layers: readonly GraphicLayer[]; width?: number; height?: number; localUs?: number; onchange: (pin: GraphicPin | undefined) => void } = $props()
  let busy = $state(false)
  let error = $state('')
  async function capture() {
    const snapshot = JSON.stringify({layers,width,height,localUs})
    const pin = current()
    busy = true; error = ''
    try {
      const result = await captureGraphicPin({layers},width,height,localUs,layer.uid,pin.to,[pin.left,pin.top,pin.right,pin.bottom])
      if (snapshot !== JSON.stringify({layers,width,height,localUs})) throw new Error('Слои или время изменились; повторите захват')
      onchange(result)
    } catch (reason) { error = String(reason) } finally { busy = false }
  }
  const edges = ['left', 'top', 'right', 'bottom'] as const
  const names = ['Слева', 'Сверху', 'Справа', 'Снизу']
  function current(): GraphicPin { return layer.pin ?? { to: 'frame', left: false, top: false, right: false, bottom: false, offsets: [0, 0, 0, 0] } }
  function offset(index: number, raw: string) {
    if (!raw.trim()) return
    const value = Number(raw)
    if (!Number.isFinite(value)) return
    const pin = current(), offsets = [...pin.offsets] as [number, number, number, number]
    offsets[index] = value
    onchange({ ...pin, offsets })
  }
</script>
<details>
  <summary>Привязки</summary>
  <label>Цель<select aria-label={`Слой ${layer.uid} цель привязки`} value={layer.pin?.to === 'frame' || !layer.pin ? 'frame' : String(layer.pin.to.layer)} onchange={event => onchange({ ...current(), to: event.currentTarget.value === 'frame' ? 'frame' : { layer: Number(event.currentTarget.value) } })}>
    <option value="frame">Кадр</option>
    {#each layers.filter(value => value.uid !== layer.uid && value.uid !== 0) as target (target.uid)}<option value={String(target.uid)}>Слой {target.uid}</option>{/each}
  </select></label>
  {#each edges as edge, index}
    <div class="edge">
      <label><input type="checkbox" aria-label={`Слой ${layer.uid} привязка ${edge}`} checked={layer.pin?.[edge] ?? false} onchange={event => onchange({ ...current(), [edge]: event.currentTarget.checked })} />{names[index]}</label>
      <input type="number" step="any" aria-label={`Слой ${layer.uid} отступ ${edge}`} value={layer.pin?.offsets[index] ?? 0} onchange={event => offset(index, event.currentTarget.value)} />
    </div>
  {/each}
  <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void capture()}>Захватить текущие расстояния</button>
  {#if error}<p role="alert">{error}</p>{/if}
  <button class="btn ghost sm" type="button" disabled={!layer.pin} onclick={() => onchange(undefined)}>Удалить привязки</button>
</details>
<style>
  label, .edge { display: flex; align-items: center; gap: .4rem; }
  .edge { justify-content: space-between; margin-top: .2rem; }
  .edge input[type=number] { width: 6rem; }
</style>
