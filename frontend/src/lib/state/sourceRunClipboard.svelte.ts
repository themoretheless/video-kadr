import type {SourceRunClipboard} from '$lib/composition/sourceRunClipboard'
class ClipboardState {value=$state.raw<SourceRunClipboard|null>(null)}
/** Normalized source data only; independent of media, projects and Undo. */
export const sourceRunClipboard=new ClipboardState()
