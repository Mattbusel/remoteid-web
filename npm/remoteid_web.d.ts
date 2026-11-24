/* tslint:disable */
/* eslint-disable */

/**
 * Analyze a captured Remote ID file (pcap, pcapng or RIDF) with the real
 * crates and return the full field assessment as JSON. `{"ok":false,...}`
 * when the bytes are not a capture or carry no Remote ID.
 */
export function analyze_capture(bytes: Uint8Array): string;

/**
 * Build the one-shot "spoofing flood" demo and run the detector over it.
 *
 * `fake_count` is how many templated fake broadcasters to put in the flood
 * alongside the single genuine drone. Returns a JSON string the page renders.
 */
export function run_demo(fake_count: number): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_capture: (a: number, b: number) => [number, number];
    readonly run_demo: (a: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
