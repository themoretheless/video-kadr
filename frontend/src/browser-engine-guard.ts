export class BrowserEngineTimeoutError extends Error {
  constructor(readonly phase: string, readonly timeoutMs: number) {
    super(`FFmpeg не ответил на этапе «${phase}» за ${Math.ceil(timeoutMs / 1000)} с`)
    this.name = 'BrowserEngineTimeoutError'
  }
}

export async function boundedEnginePhase<T>(
  operation: Promise<T>,
  phase: string,
  timeoutMs: number,
  terminate: () => void,
): Promise<T> {
  let timer: number | undefined
  try {
    return await Promise.race([
      operation,
      new Promise<T>((_, reject) => {
        timer = window.setTimeout(() => {
          terminate()
          reject(new BrowserEngineTimeoutError(phase, timeoutMs))
        }, timeoutMs)
      }),
    ])
  } finally {
    if (timer !== undefined) window.clearTimeout(timer)
  }
}
