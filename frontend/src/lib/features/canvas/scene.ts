export const CANVAS_SCENE_LAYERS = ['media', 'guides', 'overlays', 'handles'] as const
export type CanvasSceneLayer = typeof CANVAS_SCENE_LAYERS[number]

export interface CanvasSceneNode {
  readonly id: string
  readonly layer: CanvasSceneLayer
  readonly interactive: boolean
  readonly hidden?: boolean
}

export interface CanvasScene {
  readonly layers: Readonly<Record<CanvasSceneLayer, readonly CanvasSceneNode[]>>
}

export function buildCanvasScene(nodes: readonly CanvasSceneNode[]): CanvasScene {
  const layers: Record<CanvasSceneLayer, CanvasSceneNode[]> = {
    media: [], guides: [], overlays: [], handles: [],
  }
  for (const node of nodes) layers[node.layer].push(node)
  return { layers }
}

export function hitTestCandidates(scene: CanvasScene): readonly CanvasSceneNode[] {
  return [...scene.layers.overlays, ...scene.layers.handles]
    .filter((node) => node.interactive && !node.hidden)
    .reverse()
}

export function sceneLayerAttributes(layer: CanvasSceneLayer, interactive = false): Record<string, string> {
  return {
    'data-scene-layer': layer,
    'data-hit-test': interactive ? 'interactive' : 'passthrough',
  }
}
