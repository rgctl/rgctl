/// <reference types="vite/client" />

declare module "../wasm/rbuilder_wasm.js" {
  export default function init(): Promise<void>;
  export class EngineContext {
    constructor(bytes: Uint8Array);
    readonly node_count: number;
    readonly edge_count: number;
    readonly schema_version: number;
    readonly digest: string;
  }
  export function parseCfgDetail(bytes: Uint8Array): string;
}
