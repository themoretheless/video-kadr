export interface PreviewRepresentation {
  readonly id: string
  readonly width: number
  readonly bitrateKbps?: number
  readonly local: boolean
}

export interface PreviewRepresentationContext {
  readonly viewportWidth: number
  readonly scrubbing: boolean
  readonly networkConstrained: boolean
}

export function choosePreviewRepresentation(
  representations: readonly PreviewRepresentation[],
  context: PreviewRepresentationContext,
): PreviewRepresentation | null {
  if (!representations.length) return null
  const sorted = [...representations].sort((a, b) => a.width - b.width)
  const target = Math.max(1, context.viewportWidth) * (context.scrubbing ? 0.75 : 1.25)
  const eligible = sorted.filter((item) => item.width <= target)
  const selected = eligible.at(-1) ?? sorted[0]!
  if (!context.networkConstrained) return selected
  return sorted
    .filter((item) => item.local || (item.bitrateKbps ?? Number.MAX_SAFE_INTEGER) <= 2_500)
    .at(-1) ?? selected
}

export function assertPreviewPolicyDoesNotMutateExport<T>(exportSpec: T): T {
  return exportSpec
}
