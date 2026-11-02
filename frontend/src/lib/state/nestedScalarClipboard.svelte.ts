import type { NativeScalarClipboard } from '$lib/composition/nativeScalarClipboard'
class ClipboardState { value=$state.raw<NativeScalarClipboard|null>(null) }
/** An authored numeric snapshot, independent of project/history and media assets. */
export const nestedScalarClipboard=new ClipboardState()
