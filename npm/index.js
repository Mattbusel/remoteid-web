import init, { analyze_capture, run_demo, initSync } from './remoteid_web.js';

/**
 * Load the WASM module. In the browser pass nothing (fetches the .wasm next
 * to the bundle) or a URL/Response; in Node pass the file bytes:
 *
 *   import { readFile } from 'node:fs/promises';
 *   await load(await readFile(new URL('./remoteid_web_bg.wasm', import.meta.url)));
 */
export async function load(moduleOrPath) {
    await init(moduleOrPath === undefined ? undefined : { module_or_path: moduleOrPath });
}

/** Synchronous load from bytes or a WebAssembly.Module (Node-friendly). */
export function loadSync(bytesOrModule) {
    initSync({ module: bytesOrModule });
}

/**
 * Analyze a captured Remote ID file (pcap, pcapng or RIDF).
 * Returns the parsed assessment object; `{ ok: false, error }` when the
 * bytes are not a capture or carry no Remote ID.
 */
export function analyzeCapture(bytes) {
    return JSON.parse(analyze_capture(bytes));
}

/** The spoofing-flood demo: N fake broadcasters, one genuine drone. */
export function runDemo(fakeCount) {
    return JSON.parse(run_demo(fakeCount));
}

export { analyze_capture, run_demo };
