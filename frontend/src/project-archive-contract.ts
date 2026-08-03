/** Pure UX/domain contract for a future portable-project archive.
 *
 * This module deliberately has no browser, storage, network or Vue imports so
 * backend and browser implementations can expose the same planning semantics.
 */

export const PROJECT_ARCHIVE_SCHEMA_VERSION = 1 as const

export interface ProjectArchiveExportOptions {
  /** Exact source bytes needed for export on another device. */
  includeOriginalMedia: boolean
  /** Rebuildable preview artifacts. Independent from original inclusion. */
  includeProxies: boolean
}

export type ProjectArchiveAssetRole = 'original' | 'proxy' | 'lut'
export type ProjectArchiveAssetAvailability = 'ready' | 'session' | 'offline' | 'permission-required'

export interface ProjectArchiveAssetCandidate {
  id: string
  role: ProjectArchiveAssetRole
  displayName: string
  sizeBytes: number
  availability: ProjectArchiveAssetAvailability
  /** LUTs and other non-rebuildable edit dependencies must be carried. */
  requiredForEdit?: boolean
}

export type ProjectArchivePlanIssueCode =
  | 'invalid_asset'
  | 'missing_original'
  | 'missing_required_asset'
  | 'proxy_skipped'
  | 'relink_required'
  | 'proxy_only'

export interface ProjectArchivePlanIssue {
  code: ProjectArchivePlanIssueCode
  assetId?: string
  displayName?: string
}

export interface ProjectArchiveExportPlan {
  includedAssetIds: string[]
  estimatedBytes: number
  blockers: ProjectArchivePlanIssue[]
  warnings: ProjectArchivePlanIssue[]
}

/** Decide what the export UI may promise before any hashing or packaging I/O. */
export function planProjectArchiveExport(
  projectBytes: number,
  options: ProjectArchiveExportOptions,
  assets: readonly ProjectArchiveAssetCandidate[],
): ProjectArchiveExportPlan {
  if (!Number.isSafeInteger(projectBytes) || projectBytes <= 0) {
    throw new Error('project archive size estimate is invalid')
  }
  const includedAssetIds: string[] = []
  const blockers: ProjectArchivePlanIssue[] = []
  const warnings: ProjectArchivePlanIssue[] = []
  const seen = new Set<string>()
  let estimatedBytes = projectBytes

  for (const asset of assets) {
    if (
      !asset.id.trim()
      || seen.has(asset.id)
      || !asset.displayName.trim()
      || !Number.isSafeInteger(asset.sizeBytes)
      || asset.sizeBytes <= 0
    ) {
      blockers.push({ code: 'invalid_asset', assetId: asset.id || undefined, displayName: asset.displayName || undefined })
      continue
    }
    seen.add(asset.id)

    const selected = asset.role === 'lut' && asset.requiredForEdit === true
      || asset.role === 'original' && options.includeOriginalMedia
      || asset.role === 'proxy' && options.includeProxies

    if (!selected) {
      if (asset.role === 'original') {
        warnings.push({ code: 'relink_required', assetId: asset.id, displayName: asset.displayName })
      }
      continue
    }

    if (asset.availability !== 'ready' && asset.availability !== 'session') {
      const issue = { assetId: asset.id, displayName: asset.displayName }
      if (asset.role === 'proxy') warnings.push({ code: 'proxy_skipped', ...issue })
      else if (asset.role === 'original') blockers.push({ code: 'missing_original', ...issue })
      else blockers.push({ code: 'missing_required_asset', ...issue })
      continue
    }

    includedAssetIds.push(asset.id)
    estimatedBytes += asset.sizeBytes
    if (!Number.isSafeInteger(estimatedBytes)) {
      blockers.push({ code: 'invalid_asset', assetId: asset.id, displayName: asset.displayName })
    }
  }

  if (!options.includeOriginalMedia && options.includeProxies) warnings.push({ code: 'proxy_only' })
  return { includedAssetIds, estimatedBytes, blockers, warnings }
}

export type ProjectArchiveProjectConflict = 'absent' | 'identical' | 'diverged'
export type ProjectArchiveProjectAction = 'create' | 'reuse' | 'copy'

/** Destructive replacement is intentionally never an implicit import action. */
export function defaultProjectArchiveAction(conflict: ProjectArchiveProjectConflict): ProjectArchiveProjectAction {
  if (conflict === 'absent') return 'create'
  if (conflict === 'identical') return 'reuse'
  return 'copy'
}

export type ProjectArchiveMediaConflict = 'absent' | 'same-fingerprint' | 'different-fingerprint'
export type ProjectArchiveMediaAction = 'create' | 'reuse' | 'rekey'

/** An imported asset must never overwrite different bytes behind a local ID. */
export function defaultProjectArchiveMediaAction(conflict: ProjectArchiveMediaConflict): ProjectArchiveMediaAction {
  if (conflict === 'absent') return 'create'
  if (conflict === 'same-fingerprint') return 'reuse'
  return 'rekey'
}
