import { describe, expect, it } from 'vitest'
import { reviewTokenFromPath } from './reviewRoute'

describe('reviewTokenFromPath', () => {
  it('accepts the opaque share-token alphabet and an optional trailing slash', () => {
    expect(reviewTokenFromPath('/review/123e4567-e89b-12d3-a456.opaque')).toBe('123e4567-e89b-12d3-a456.opaque')
    expect(reviewTokenFromPath('/review/a-b.c/')).toBe('a-b.c')
  })

  it('rejects extra segments, empty tokens and encoded separators', () => {
    expect(reviewTokenFromPath('/review/')).toBeNull()
    expect(reviewTokenFromPath('/review/token/edit')).toBeNull()
    expect(reviewTokenFromPath('/review/token%2Fedit')).toBeNull()
  })
})
