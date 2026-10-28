/// <reference types="vite/client" />

declare module "../wasm/rgctl_wasm.js" {
  export default function init(): Promise<void>;
  export class EngineContext {
    constructor(bytes: Uint8Array);
    readonly node_count: number;
    readonly edge_count: number;
    readonly schema_version: number;
    readonly digest: string;
    expandIndices(indices: Uint32Array, typeMask: number): string;
    listNodes(typeMask: number, offset: number, limit: number): string;
    blastRadius(startIndex: number, maxDepth: number): string;
    /** Columnar index for a node UUID, or `undefined` if absent. */
    indexOfUuid(uuid: string): number | undefined;
    /** Blast radius keyed by stable UUID (taint / Focus subjects). */
    blastRadiusByUuid(uuid: string, maxDepth: number): string;
  }
  export function parseCfgDetail(bytes: Uint8Array): string;
}
