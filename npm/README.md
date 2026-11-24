# remoteid-web

Drone Remote ID (ASTM F3411-22a / ASD-STAN EN 4709-002) capture analysis, compiled from the
Rust [`remoteid`](https://crates.io/crates/remoteid) crates to WebAssembly. Runs in the browser
and in Node — no native dependencies, no network access.

Feed it a capture file (pcap, pcapng or [RIDF](https://github.com/Mattbusel/remoteid/blob/main/docs/ridf.md))
and get back a full field assessment: decoded messages, per-transmitter trust verdicts
(authenticated / plausible / suspicious / spoofed, with reasons), spoofing-flood detection,
and the genuine track when one stands out.

## Install

```sh
npm install remoteid-web
```

## Use

```js
import { readFile } from 'node:fs/promises';
import { loadSync, analyzeCapture } from 'remoteid-web';

// Node: load the bundled .wasm explicitly.
loadSync(await readFile(new URL(
  './node_modules/remoteid-web/remoteid_web_bg.wasm', import.meta.url)));

const report = analyzeCapture(new Uint8Array(await readFile('capture.pcap')));
console.log(report.flood.level, report.genuine?.uas_id);
// -> "flooded" "1581CREAL7A4F2C91"
```

Browser (bundler or native ESM):

```js
import init, { analyze_capture } from 'remoteid-web/remoteid_web.js';
await init(); // fetches the .wasm next to the bundle
const report = JSON.parse(analyze_capture(bytes));
```

`analyzeCapture` / `analyze_capture` never throw on bad input: unparsable bytes return
`{ ok: false, error: 'unknown_format' | 'corrupt' | 'no_remote_id', detail }`.

`runDemo(n)` / `run_demo(n)` builds a synthetic spoofing flood with `n` fake broadcasters
and one genuine drone hidden in it, then runs the detector over it — a self-contained
way to see what the engine reports.

## What the verdict looks at

Kinematic plausibility (jumps, impossible speed, reported vs. flown track), timestamp and
counter continuity, Wi-Fi TSF behaviour, agreement between transports and transmitters
claiming one identity, fingerprints of two public spoofing tools, and DRIP (RFC 9575)
Ed25519 signatures. Verdicts distinguish *detected anomalies* from confirmed spoofing;
see the [trust model](https://github.com/Mattbusel/remoteid/blob/main/docs/trust.md).

## License

MIT OR Apache-2.0, same as the underlying crates.
