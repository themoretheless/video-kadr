/** One-shot UI navigation, independent of document/history mutations. */
export interface ActivityReveal {id:string;workspaceId:string;mode:'ordinary'|'nested';compositionId:string;trackId:string;clipId:string|null}
export const activityReveal=$state<{request:ActivityReveal|null}>({request:null})
