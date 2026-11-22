// Loads the WebAssembly detector and drives the page. All computation
// (building the flood, decoding every broadcast, grading the field) happens
// inside run_demo, which is the real remoteid / remoteid-sentinel code compiled
// to wasm. This file only animates the result. Nothing here touches a radio or
// the network.

import init, { run_demo } from "./remoteid_web.js";

const $ = (id) => document.getElementById(id);
const scope = $("scope");
const ctx = scope.getContext("2d");
const W = scope.width, H = scope.height;

let ready = false;
init().then(() => {
  ready = true;
  $("status").textContent = "Detector loaded. Press the button.";
  drawIdle();
});
$("status").textContent = "Loading detector…";

$("run").addEventListener("click", () => {
  if (!ready) return;
  const n = parseInt($("count").value, 10);
  runOnce(n);
});

// ---- geometry -------------------------------------------------------------

// Deterministic pseudo-random in [0,1) from an integer, so the fake scatter is
// stable across redraws of the same run.
function rnd(seed) {
  let x = (seed * 2654435761) >>> 0;
  x ^= x >>> 15; x = (x * 2246822519) >>> 0;
  x ^= x >>> 13; x = (x * 3266489917) >>> 0;
  x ^= x >>> 16;
  return (x >>> 0) / 4294967296;
}

function makeProjector(data) {
  const pts = [];
  if (data.genuine) for (const p of data.genuine.path) pts.push(p);
  for (const t of data.tracks) if (t.lat != null && t.lon != null) pts.push([t.lat, t.lon]);
  let latMin = Infinity, latMax = -Infinity, lonMin = Infinity, lonMax = -Infinity;
  for (const [la, lo] of pts) {
    latMin = Math.min(latMin, la); latMax = Math.max(latMax, la);
    lonMin = Math.min(lonMin, lo); lonMax = Math.max(lonMax, lo);
  }
  // Pad so nothing sits on the edge and the fake cluster has room to spread.
  const latPad = Math.max((latMax - latMin) * 0.25, 0.0008);
  const lonPad = Math.max((lonMax - lonMin) * 0.25, 0.0008);
  latMin -= latPad; latMax += latPad; lonMin -= lonPad; lonMax += lonPad;
  const pad = 46;
  const sx = (W - 2 * pad) / Math.max(lonMax - lonMin, 1e-9);
  const sy = (H - 2 * pad) / Math.max(latMax - latMin, 1e-9);
  return (la, lo) => [
    pad + (lo - lonMin) * sx,
    pad + (latMax - la) * sy, // north up
  ];
}

// ---- rendering ------------------------------------------------------------

function drawGrid() {
  ctx.clearRect(0, 0, W, H);
  ctx.save();
  ctx.strokeStyle = "rgba(74,168,255,0.10)";
  ctx.lineWidth = 1;
  const cx = W / 2, cy = H / 2;
  for (let r = 60; r < Math.max(W, H); r += 60) {
    ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.stroke();
  }
  ctx.beginPath(); ctx.moveTo(0, cy); ctx.lineTo(W, cy);
  ctx.moveTo(cx, 0); ctx.lineTo(cx, H); ctx.stroke();
  ctx.restore();
}

function drawIdle() {
  drawGrid();
  ctx.save();
  ctx.fillStyle = "rgba(134,153,175,0.75)";
  ctx.font = "15px ui-sans-serif, system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("Press “Simulate a spoofing flood” to generate the airspace.", W / 2, H / 2);
  ctx.restore();
}

// Precomputed per-fake screen positions (cluster center + stable jitter).
function layoutFakes(data, project) {
  const fakes = data.tracks.filter((t) => !t.is_genuine && t.lat != null);
  // All fakes claim one position; spread them in screen space so the cluster
  // is visible. The overlap itself is a detection signal, noted in the panel.
  let cx = 0, cy = 0;
  const base = fakes.map((t) => project(t.lat, t.lon));
  for (const [x, y] of base) { cx += x; cy += y; }
  cx /= base.length || 1; cy /= base.length || 1;
  const R = 78;
  return fakes.map((t, i) => {
    const a = rnd(i * 2 + 1) * Math.PI * 2;
    const rad = Math.sqrt(rnd(i * 2 + 7)) * R;
    return { t, x: cx + Math.cos(a) * rad, y: cy + Math.sin(a) * rad, cx, cy, R };
  });
}

function render(data, project, fakeLayout, reveal, pulse) {
  drawGrid();

  // Fake cluster boundary.
  if (fakeLayout.length) {
    const { cx, cy, R } = fakeLayout[0];
    ctx.save();
    ctx.setLineDash([5, 5]);
    ctx.strokeStyle = "rgba(255,93,108,0.35)";
    ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.arc(cx, cy, R + 10, 0, Math.PI * 2); ctx.stroke();
    ctx.restore();
  }

  // Fakes (progressive reveal).
  const shown = Math.floor(fakeLayout.length * reveal);
  ctx.save();
  ctx.fillStyle = "rgba(255,93,108,0.82)";
  for (let i = 0; i < shown; i++) {
    const f = fakeLayout[i];
    ctx.beginPath(); ctx.arc(f.x, f.y, 2.6, 0, Math.PI * 2); ctx.fill();
  }
  ctx.restore();

  if (reveal < 1 || !data.genuine) return;

  // Real flight path.
  const path = data.genuine.path.map(([la, lo]) => project(la, lo));
  if (path.length > 1) {
    ctx.save();
    ctx.setLineDash([6, 5]);
    ctx.strokeStyle = "rgba(57,217,138,0.85)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(path[0][0], path[0][1]);
    for (const [x, y] of path.slice(1)) ctx.lineTo(x, y);
    ctx.stroke();
    ctx.restore();
  }

  // Real drone, highlighted with a pulsing ring.
  const [gx, gy] = project(data.genuine.lat, data.genuine.lon);
  ctx.save();
  const ringR = 12 + 8 * (0.5 + 0.5 * Math.sin(pulse / 260));
  ctx.strokeStyle = "rgba(57,217,138,0.6)";
  ctx.lineWidth = 2;
  ctx.beginPath(); ctx.arc(gx, gy, ringR, 0, Math.PI * 2); ctx.stroke();
  ctx.fillStyle = "#39d98a";
  ctx.shadowColor = "#39d98a"; ctx.shadowBlur = 14;
  ctx.beginPath(); ctx.arc(gx, gy, 5.5, 0, Math.PI * 2); ctx.fill();
  ctx.restore();

  // Callout.
  ctx.save();
  ctx.fillStyle = "#39d98a";
  ctx.font = "600 13px ui-sans-serif, system-ui, sans-serif";
  ctx.textAlign = gx > W - 160 ? "right" : "left";
  const dx = gx > W - 160 ? -14 : 14;
  ctx.fillText("REAL DRONE", gx + dx, gy - 10);
  ctx.fillStyle = "rgba(228,235,243,0.85)";
  ctx.font = "12px ui-monospace, Menlo, Consolas, monospace";
  ctx.fillText(data.genuine.uas_id || data.genuine.label, gx + dx, gy + 7);
  ctx.restore();
}

// ---- run ------------------------------------------------------------------

function runOnce(n) {
  const btn = $("run");
  btn.disabled = true;
  $("panel").hidden = true;
  $("legend").hidden = true;
  $("status").textContent = `Generating ${n} fake broadcasts + 1 real drone…`;
  drawGrid();

  // Let the status paint before the (fast) synchronous wasm call.
  requestAnimationFrame(() => {
    const t0 = performance.now();
    const data = JSON.parse(run_demo(n));
    const ms = Math.round(performance.now() - t0);

    const project = makeProjector(data);
    const fakeLayout = layoutFakes(data, project);

    $("status").textContent =
      `Decoded ${data.track_count} tracks in ${ms} ms. Detector running…`;

    const start = performance.now();
    const revealMs = 900;
    function frame(now) {
      const reveal = Math.min((now - start) / revealMs, 1);
      render(data, project, fakeLayout, reveal, now - start);
      if (reveal < 1) {
        requestAnimationFrame(frame);
      } else {
        // Keep pulsing the real drone.
        render(data, project, fakeLayout, 1, now - start);
        requestAnimationFrame(frame2);
      }
    }
    let pulseStart = performance.now();
    function frame2(now) {
      render(data, project, fakeLayout, 1, now - pulseStart);
      requestAnimationFrame(frame2);
    }
    requestAnimationFrame(frame);

    showPanel(data, ms);
    $("legend").hidden = false;
    $("status").textContent =
      `${data.track_count} tracks, ${n} fakes + 1 real — decoded and graded in ${ms} ms.`;
    btn.disabled = false;
  });
}

function showPanel(data, ms) {
  const panel = $("panel");
  const v = data.flood;
  const level = v.level;
  const pct = Math.round(v.confidence * 100);
  $("verdict").className = "verdict " + level;
  $("verdict").innerHTML =
    `Airspace: ${level.toUpperCase()}` +
    `<span class="conf">Flood confidence ${pct}% &middot; ${data.track_count} tracks analyzed</span>`;

  const g = data.genuine;
  const rc = $("realcard");
  if (g) {
    const reasons = g.reasons.map((r) => `<li>${escapeHtml(r)}</li>`).join("");
    rc.hidden = false;
    rc.innerHTML =
      `<h3>One real drone found</h3>` +
      `<div class="id">${escapeHtml(g.uas_id || g.label)}</div>` +
      `<div class="meta">radio ${escapeHtml(g.label)} &middot; ` +
      `RSSI ${g.rssi_dbm ?? "?"} dBm &middot; real-score ${g.real_score.toFixed(2)} &middot; ` +
      `per-unit verdict: ${escapeHtml(g.verdict)}</div>` +
      `<strong style="font-size:.82rem;color:#8699af">Why this one:</strong>` +
      `<ul>${reasons}</ul>`;
  } else {
    rc.hidden = false;
    rc.innerHTML = `<h3>No track survived as real</h3>`;
  }

  const sigs = $("sigs");
  const hist = data.signature_histogram;
  const max = hist.reduce((m, [, c]) => Math.max(m, c), 1);
  sigs.innerHTML =
    `<h3>Why the rest are fake (${data.track_count - (g ? 1 : 0)} tracks)</h3>` +
    hist
      .map(([name, c]) => {
        const w = Math.round((c / max) * 100);
        return (
          `<div class="sigrow"><span>${escapeHtml(prettySig(name))}</span>` +
          `<span class="n">${c}</span>` +
          `<span class="bar"><i style="width:${w}%"></i></span></div>`
        );
      })
      .join("");

  panel.hidden = false;
}

function prettySig(name) {
  const map = {
    one_source_many_ids: "One source, many IDs (fan-out)",
    tight_rssi_position_cluster: "Tight RSSI + position cluster",
    templated_serial: "Templated / sequential serials",
    frozen_or_identical_counter: "Frozen / lockstep counters",
    impossible_kinematics: "Impossible kinematics",
    shared_operator_position: "Shared operator (pilot) position",
    per_unit_spoofed_or_suspicious: "Graded spoofed / suspicious",
    known_spoofer_fingerprint: "Known spoofer fingerprint",
    count_spike: "Sudden spawn (count spike)",
  };
  return map[name] || name;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );
}
