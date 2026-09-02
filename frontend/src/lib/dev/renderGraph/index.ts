import { RENDER_GRAPH_DEV_MARKER, topologicalRenderOrder, type RenderGraphNode } from './model.js'

export function mountRenderGraphDevtools(nodes: readonly RenderGraphNode[] = []): void {
  const root = document.createElement('details')
  root.dataset.devtool = RENDER_GRAPH_DEV_MARKER
  root.style.cssText = 'position:fixed;right:8px;bottom:8px;z-index:9999;background:var(--panel);color:var(--text);border:1px solid var(--border);padding:8px;font:12px monospace'
  const summary = document.createElement('summary')
  summary.textContent = `Render DAG (${nodes.length})`
  const output = document.createElement('pre')
  output.textContent = nodes.length ? topologicalRenderOrder(nodes).join('\n→ ') : 'No active render graph'
  root.append(summary, output)
  document.body.append(root)
}
