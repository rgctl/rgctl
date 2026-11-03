/**
 * wasm-bindgen glue shipped under `dashboard/wasm/` — CFG archive + blast engine.
 *
 * `dashboard/wasm/` is generated (`wasm-pack`) and gitignored, so a fresh checkout has no `.d.ts` beside the
 * glue and these declarations are what typechecks against. They must therefore match
 * `crates/rgctl-wasm/src/lib.rs` — a method added there and not here is a type error that only appears once
 * someone has run `wasm-pack`, which is exactly the silent gap that shipped an engine missing
 * `blastRadiusByUuid`.
 */
declare module "../../wasm/rgctl_wasm.js" {
  export default function __wbg_init(
    module_or_path?: { module_or_path: unknown } | unknown,
  ): Promise<unknown>;
  export function parseCfgDetail(bytes: Uint8Array): string;
  export function initSync(module: unknown): unknown;
  export class EngineContext {
    constructor(bytes: Uint8Array);
    readonly node_count: number;
    readonly edge_count: number;
    readonly schema_version: number;
    readonly digest: string;
    expandIndices(indices: Uint32Array, typeMask: number): string;
    listNodes(typeMask: number, offset: number, limit: number): string;
    blastRadius(startIndex: number, maxDepth: number): string;
    indexOfUuid(uuid: string): number | undefined;
    blastRadiusByUuid(uuid: string, maxDepth: number): string;
  }
}
