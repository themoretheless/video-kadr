const REVIEW_ROUTE = /^\/review\/([A-Za-z0-9.-]{1,128})\/?$/

export function reviewTokenFromPath(pathname: string): string | null {
  const encoded = REVIEW_ROUTE.exec(pathname)?.[1]
  if (!encoded) return null
  try {
    const token = decodeURIComponent(encoded)
    return /^[A-Za-z0-9.-]{1,128}$/.test(token) ? token : null
  } catch {
    return null
  }
}
