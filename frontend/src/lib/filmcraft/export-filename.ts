export function filmcraftDownloadName(result: unknown, id: string, extension: string) {
  const name = result && typeof result === 'object' && 'outputName' in result && typeof result.outputName === 'string'
    ? result.outputName : id
  return `${name}.${extension}`
}
