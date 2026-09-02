export const RENDER_GRAPH_DEV_MARKER = 'video-kadr-render-dag-devtools'

export interface RenderGraphNode {
  readonly id: string
  readonly label: string
  readonly dependencies: readonly string[]
}

export function topologicalRenderOrder(nodes: readonly RenderGraphNode[]): readonly string[] {
  const pending = new Map(nodes.map((node) => [node.id, new Set(node.dependencies)]))
  const order: string[] = []
  while (pending.size) {
    const ready = [...pending.entries()].filter(([, dependencies]) => dependencies.size === 0)
      .map(([id]) => id).sort()
    if (!ready.length) throw new Error('Render graph contains a cycle or missing dependency')
    for (const id of ready) {
      pending.delete(id)
      order.push(id)
      for (const dependencies of pending.values()) dependencies.delete(id)
    }
  }
  return order
}
