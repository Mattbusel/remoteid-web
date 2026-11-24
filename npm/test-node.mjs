import { readFile } from 'node:fs/promises';
import { loadSync, analyzeCapture, runDemo } from './index.js';

loadSync(await readFile('./remoteid_web_bg.wasm'));

const cap = await readFile('C:/Windows/Temp/sample-flood.pcap');
const report = analyzeCapture(new Uint8Array(cap));
console.log('frames:', report.frames, '| flood.level:', report.flood.level,
    '| confidence:', report.flood.confidence, '| genuine:', report.genuine?.uas_id);
if (report.frames !== 3280 || report.flood.level !== 'flooded' || !report.genuine?.uas_id)
    throw new Error('unexpected verdict');
const err = analyzeCapture(new Uint8Array([1, 2, 3]));
if (!err.error) throw new Error('expected error object for garbage input');
console.log('garbage input ->', JSON.stringify(err));
const demo = runDemo(10);
console.log('run_demo keys:', Object.keys(demo).slice(0, 5).join(','));
console.log('NODE PACKAGE VERIFIED');
