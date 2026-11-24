export declare function load(moduleOrPath?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;
export declare function loadSync(bytesOrModule: BufferSource | WebAssembly.Module): void;
export declare function analyzeCapture(bytes: Uint8Array): any;
export declare function runDemo(fakeCount: number): any;
export { analyze_capture, run_demo } from './remoteid_web.js';
