export interface Command<T extends object> {
  readonly mergeKey: string
  apply(target: T): T
  invert(): Command<T>
  merge(next: Command<T>): Command<T> | null
}

export interface FieldChange<T extends object, K extends keyof T = keyof T> {
  readonly key: K
  readonly before: T[K]
  readonly after: T[K]
}

/** A semantic edit command stores only changed top-level fields. Nested values
 * are cloned, so later Vue mutations cannot rewrite history retroactively. */
export class PatchCommand<T extends object> implements Command<T> {
  readonly changes: readonly FieldChange<T>[]

  constructor(
    readonly mergeKey: string,
    changes: readonly FieldChange<T>[],
  ) {
    this.changes = changes.map((change) => ({
      key: change.key,
      before: cloneValue(change.before),
      after: cloneValue(change.after),
    }))
  }

  static between<T extends object>(before: T, after: T, mergeKey: string): PatchCommand<T> | null {
    const keys = new Set([...Object.keys(before), ...Object.keys(after)] as (keyof T)[])
    const changes: FieldChange<T>[] = []
    for (const key of keys) {
      if (!valuesEqual(before[key], after[key])) {
        changes.push({ key, before: before[key], after: after[key] })
      }
    }
    return changes.length ? new PatchCommand(mergeKey, changes) : null
  }

  get changedKeys(): readonly (keyof T)[] {
    return this.changes.map((change) => change.key)
  }

  apply(target: T): T {
    const result = cloneValue(target) as T & Record<keyof T, T[keyof T]>
    for (const change of this.changes) {
      result[change.key] = cloneValue(change.after)
    }
    return result
  }

  invert(): PatchCommand<T> {
    return new PatchCommand(
      this.mergeKey,
      this.changes.map((change) => ({
        key: change.key,
        before: change.after,
        after: change.before,
      })),
    )
  }

  merge(next: Command<T>): PatchCommand<T> | null {
    if (!(next instanceof PatchCommand) || next.mergeKey !== this.mergeKey) return null
    const nextPatch = next as PatchCommand<T>
    const combined = new Map<keyof T, FieldChange<T>>()
    for (const change of this.changes) combined.set(change.key, change)
    for (const change of nextPatch.changes) {
      const previous = combined.get(change.key)
      combined.set(change.key, {
        key: change.key,
        before: previous?.before ?? change.before,
        after: change.after,
      })
    }
    const changes = [...combined.values()].filter(
      (change) => !valuesEqual(change.before, change.after),
    )
    return changes.length ? new PatchCommand(this.mergeKey, changes) : null
  }
}

export function cloneValue<T>(value: T): T {
  if (Array.isArray(value)) return value.map((item) => cloneValue(item)) as T
  if (value && typeof value === 'object') {
    const clone: Record<string, unknown> = {}
    for (const [key, nested] of Object.entries(value)) clone[key] = cloneValue(nested)
    return clone as T
  }
  return value
}

function valuesEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((value, index) => valuesEqual(value, right[index]))
  }
  if (isRecord(left) && isRecord(right)) {
    const leftKeys = Object.keys(left)
    const rightKeys = Object.keys(right)
    return (
      leftKeys.length === rightKeys.length &&
      leftKeys.every((key) => key in right && valuesEqual(left[key], right[key]))
    )
  }
  return false
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}
