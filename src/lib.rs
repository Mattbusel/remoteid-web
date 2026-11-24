//! Browser (wasm) demo of the receive-only Remote ID flood / spoof detector.
//!
//! The browser calls [`run_demo`]. That function, entirely in-process and with
//! no radio or network, does three things:
//!
//! 1. builds a synthetic "spoofing flood" with
//!    [`remoteid_scenario`] (hundreds of templated fake Remote ID broadcasts
//!    plus one genuine moving drone hidden among them),
//! 2. decodes every broadcast exactly as a receiver would, with
//!    [`remoteid`], and
//! 3. runs the receive-only field analyzer [`remoteid_sentinel`] over the
//!    decoded tracks to grade the flood and point at the one track most likely
//!    to be the real aircraft.
//!
//! It then returns a JSON summary the page renders. Everything here is
//! analysis of already-decoded broadcasts. This crate opens no radio, speaks
//! to no device, and has no transmit path: it never produces an RF frame, only
//! a verdict.

use remoteid::capture::{self, CapturedFrame};
use remoteid::{Frame, Message, Observation, Transmitter, Transport};
use remoteid_scenario::{generate, EmittedFrame, Scenario, ScenarioKind};
use remoteid_sentinel::{FieldAssessment, TrackScore};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use wasm_bindgen::prelude::*;

/// A short stable label for a transmitter address, for the UI.
fn transmitter_label(tx: &Transmitter) -> String {
    match tx {
        Transmitter::Ble(a) | Transmitter::Wifi(a) => {
            let kind = if matches!(tx, Transmitter::Ble(_)) {
                "ble"
            } else {
                "wifi"
            };
            format!(
                "{kind}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                a[0], a[1], a[2], a[3], a[4], a[5]
            )
        }
        Transmitter::Other(v) => format!("other:{v}"),
    }
}

/// What the page draws and lists for one track.
#[derive(Serialize)]
struct TrackJson {
    label: String,
    uas_id: Option<String>,
    /// Last position the track reported, degrees. `None` if it never sent one.
    lat: Option<f64>,
    lon: Option<f64>,
    rssi_dbm: Option<i8>,
    /// Per-unit library verdict: spoofed / suspicious / plausible / authenticated.
    verdict: String,
    /// Per-unit library reasons (snake_case), the evidence behind the verdict.
    reasons: Vec<String>,
    /// DRIP authentication state.
    auth: String,
    /// Cross-track flood / spoof signatures the sentinel raised on this track.
    signatures: Vec<String>,
    /// 0 (certainly fake) to 1 (looks like the genuine drone).
    real_score: f32,
    /// True for the single track the sentinel named as most-likely-real.
    is_genuine: bool,
}

#[derive(Serialize)]
struct FloodJson {
    level: String,
    confidence: f32,
}

#[derive(Serialize)]
struct GenuineJson {
    label: String,
    uas_id: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    rssi_dbm: Option<i8>,
    real_score: f32,
    verdict: String,
    /// The ordered reasons the sentinel chose this track as the real drone.
    reasons: Vec<String>,
    /// The track's reported flight path, [lat, lon] samples over the window.
    path: Vec<[f64; 2]>,
}

#[derive(Serialize)]
struct DemoJson {
    fake_count: u32,
    track_count: usize,
    flood: FloodJson,
    /// How many tracks carried each cross-track signature, for the summary.
    signature_histogram: Vec<(String, usize)>,
    genuine: Option<GenuineJson>,
    tracks: Vec<TrackJson>,
}

/// Last reported position and signal strength seen for a transmitter.
#[derive(Default, Clone, Copy)]
struct Display {
    lat: Option<f64>,
    lon: Option<f64>,
    rssi: Option<i8>,
}

/// Collects the last reported position per transmitter, and the full reported
/// path of one chosen transmitter (the genuine track), straight from the
/// generated frames. This is display metadata only; the detector's own input
/// is the decoded broadcast stream, not this.
fn display_map(
    frames: &[EmittedFrame],
    path_for: Option<&Transmitter>,
) -> (HashMap<Transmitter, Display>, Vec<[f64; 2]>) {
    let mut map: HashMap<Transmitter, Display> = HashMap::new();
    let mut path: Vec<[f64; 2]> = Vec::new();
    for f in frames {
        let d = map.entry(f.transmitter).or_default();
        d.rssi = Some(f.rssi_dbm);
        if let Message::Location(loc) = &f.message {
            d.lat = Some(loc.latitude);
            d.lon = Some(loc.longitude);
            if Some(&f.transmitter) == path_for {
                path.push([loc.latitude, loc.longitude]);
            }
        }
    }
    (map, path)
}

/// Build the one-shot "spoofing flood" demo and run the detector over it.
///
/// `fake_count` is how many templated fake broadcasters to put in the flood
/// alongside the single genuine drone. Returns a JSON string the page renders.
#[wasm_bindgen]
pub fn run_demo(fake_count: u32) -> String {
    // 1. Build the flood scenario: `fake_count` templated fakes that all claim
    //    one operator position, share a frozen location, carry sequential
    //    serials and lockstep counters, plus one genuine moving drone with its
    //    own serial, its own operator, honest counters and sane kinematics.
    //    Give each fake its own radio address so the flood is a field of many
    //    distinct tracks to sort through, not a single collapsed source.
    let fake_count = fake_count.clamp(1, 5000);
    let mut scenario = Scenario::new(ScenarioKind::Flood, fake_count);
    scenario.source.address_mode = remoteid_scenario::AddressMode::PerUnit;

    let frames = generate(&scenario);

    // 2. Flatten each generated message to the 25 bytes a receiver decodes,
    //    then hand the sentinel the (transmitter, observation) stream. This is
    //    exactly what a sensor replaying what it heard would feed in.
    struct Enc {
        tx: Transmitter,
        counter: u8,
        bytes: [u8; 25],
        rssi: i8,
        now_ms: u64,
    }
    let encoded: Vec<Enc> = frames
        .iter()
        .filter_map(|f| {
            f.message.encode().ok().map(|bytes| Enc {
                tx: f.transmitter,
                counter: f.counter,
                bytes,
                rssi: f.rssi_dbm,
                now_ms: f.time_us / 1000,
            })
        })
        .collect();
    let now = encoded.iter().map(|e| e.now_ms).max().unwrap_or(0) + 1;

    let observations = encoded.iter().map(|e| {
        let obs = Observation::new(Transport::Bluetooth4, Some(e.counter), &e.bytes, e.now_ms)
            .with_rssi(e.rssi);
        (e.tx, obs)
    });

    // 3. Run the receive-only field analyzer.
    let field = FieldAssessment::from_observations(observations, now);

    // Display positions per transmitter, and the genuine track's path.
    let genuine_tx = field.most_likely_real.as_ref().map(|t| t.transmitter);
    let (disp, genuine_path) = display_map(&frames, genuine_tx.as_ref());

    let genuine_tx_val = genuine_tx;
    let to_track = |t: &TrackScore| -> TrackJson {
        let d = disp.get(&t.transmitter).copied().unwrap_or_default();
        TrackJson {
            label: transmitter_label(&t.transmitter),
            uas_id: t.uas_id.clone(),
            lat: d.lat,
            lon: d.lon,
            rssi_dbm: d.rssi,
            verdict: t.per_unit.verdict.name().to_string(),
            reasons: t.per_unit.reasons.iter().map(|r| r.name().to_string()).collect(),
            auth: t.per_unit.auth.name().to_string(),
            signatures: t.fake_signals.iter().map(|s| s.name().to_string()).collect(),
            real_score: t.real_score,
            is_genuine: Some(t.transmitter) == genuine_tx_val,
        }
    };

    let tracks: Vec<TrackJson> = field.tracks.iter().map(to_track).collect();

    // Signature histogram across the field.
    let mut hist: HashMap<&'static str, usize> = HashMap::new();
    for t in &field.tracks {
        for s in t.fake_signals.iter() {
            *hist.entry(s.name()).or_insert(0) += 1;
        }
    }
    let mut signature_histogram: Vec<(String, usize)> =
        hist.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    signature_histogram.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let genuine = field.most_likely_real.as_ref().map(|t| {
        let d = disp.get(&t.transmitter).copied().unwrap_or_default();
        GenuineJson {
            label: transmitter_label(&t.transmitter),
            uas_id: t.uas_id.clone(),
            lat: d.lat,
            lon: d.lon,
            rssi_dbm: d.rssi,
            real_score: t.real_score,
            verdict: t.per_unit.verdict.name().to_string(),
            reasons: t.genuine_reasons.clone(),
            path: genuine_path,
        }
    });

    let out = DemoJson {
        fake_count,
        track_count: field.tracks.len(),
        flood: FloodJson {
            level: field.flood.level.name().to_string(),
            confidence: field.flood.confidence,
        },
        signature_histogram,
        genuine,
        tracks,
    };

    serde_json::to_string(&out).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

// ---------------------------------------------------------------------------
// Capture analysis: the incident-report engine.
//
// `analyze_capture` runs the same decode + grade + field-analysis pipeline as
// `run_demo`, but over a capture file the caller supplies (pcap, pcapng or
// RIDF: what `remoteid::capture::frames` reads). It is the analysis behind
// the Tensorust Drone Incident Report: the free teaser shows the verdict, the
// paid report shows every track, signature and reason below. Receive-only, as
// everywhere in this crate: bytes in, verdict out, no radio and no transmit
// path.
// ---------------------------------------------------------------------------

/// Per-transmitter detail for the report: everything the decoded messages
/// themselves say about one on-air source, plus its detection result.
#[derive(Serialize)]
struct CaptureTrackJson {
    label: String,
    /// Primary UAS ID (the track's current Basic ID), if any.
    uas_id: Option<String>,
    /// Every distinct UAS ID this source broadcast, capped.
    uas_ids: Vec<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    rssi_dbm: Option<i8>,
    verdict: String,
    reasons: Vec<String>,
    auth: String,
    signatures: Vec<String>,
    real_score: f32,
    is_genuine: bool,
    /// Frames received from this source, and the first/last capture times.
    frames: u64,
    first_ms: u64,
    last_ms: u64,
    operator_lat: Option<f64>,
    operator_lon: Option<f64>,
    operator_id: Option<String>,
    self_id: Option<String>,
    /// Reported positions in capture order, decimated to a bounded length.
    path: Vec<[f64; 2]>,
}

#[derive(Serialize)]
struct CaptureJson {
    frames: usize,
    transmitters: usize,
    /// Distinct UAS identities observed across the whole capture.
    identities: usize,
    /// Capture time between the first and last Remote ID frame.
    window_ms: u64,
    /// The capture clock counts from the Unix epoch (timestamps are real).
    utc_clock: bool,
    /// Frame count per radio link (bluetooth4, wifi_beacon, ...).
    transports: Vec<(String, usize)>,
    /// Decoded message count per F3411 message type.
    messages: Vec<(String, usize)>,
    flood: FloodJson,
    signature_histogram: Vec<(String, usize)>,
    genuine: Option<GenuineJson>,
    tracks: Vec<CaptureTrackJson>,
}

/// The failure answers the report flow distinguishes: `unknown_format` and
/// `corrupt` mean the upload is not a readable capture; `no_remote_id` means
/// it is a capture but carries no Remote ID, so there is nothing to report on
/// and no paid report is offered.
fn capture_error(code: &str, detail: String) -> String {
    serde_json::to_string(&serde_json::json!({ "ok": false, "error": code, "detail": detail }))
        .unwrap_or_else(|_| format!("{{\"ok\":false,\"error\":\"{code}\"}}"))
}

/// A printable rendering of a fixed-width, NUL-padded id field.
fn id_text(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let text = String::from_utf8_lossy(&raw[..end]);
    if text.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        text.trim_end().to_string()
    } else {
        raw[..end].iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// What the decoded messages of one source say, gathered for the report.
#[derive(Default, Clone)]
struct TrackFacts {
    frames: u64,
    first_ms: u64,
    last_ms: u64,
    lat: Option<f64>,
    lon: Option<f64>,
    rssi: Option<i8>,
    uas_ids: BTreeSet<String>,
    operator_id: Option<String>,
    operator_lat: Option<f64>,
    operator_lon: Option<f64>,
    self_id: Option<String>,
    path: Vec<[f64; 2]>,
}

/// One frame's decoded messages folded into the per-source facts, the
/// message-type histogram and the identity set.
fn fold_frame(
    f: &CapturedFrame,
    facts: &mut TrackFacts,
    hist: &mut HashMap<&'static str, usize>,
    identities: &mut BTreeSet<String>,
) {
    let ms = f.time_us / 1000;
    facts.frames += 1;
    if facts.frames == 1 {
        facts.first_ms = ms;
    }
    facts.last_ms = ms;
    if let Some(r) = f.rssi_dbm {
        facts.rssi = Some(r);
    }
    let Ok(frame) = Frame::decode(&f.payload) else { return };
    frame.for_each_message(|m| {
        match &m {
            Message::BasicId(b) => {
                *hist.entry("basic_id").or_insert(0) += 1;
                let id = id_text(&b.uas_id);
                if !id.is_empty() {
                    facts.uas_ids.insert(id.clone());
                    identities.insert(id);
                }
            }
            Message::Location(l) => {
                *hist.entry("location").or_insert(0) += 1;
                if l.latitude != 0.0 || l.longitude != 0.0 {
                    facts.lat = Some(l.latitude);
                    facts.lon = Some(l.longitude);
                    facts.path.push([l.latitude, l.longitude]);
                }
            }
            Message::Auth(_) => *hist.entry("auth").or_insert(0) += 1,
            Message::SelfId(s) => {
                *hist.entry("self_id").or_insert(0) += 1;
                let text = id_text(&s.description);
                if !text.is_empty() {
                    facts.self_id = Some(text);
                }
            }
            Message::System(s) => {
                *hist.entry("system").or_insert(0) += 1;
                if s.operator_latitude != 0.0 || s.operator_longitude != 0.0 {
                    facts.operator_lat = Some(s.operator_latitude);
                    facts.operator_lon = Some(s.operator_longitude);
                }
            }
            Message::OperatorId(o) => {
                *hist.entry("operator_id").or_insert(0) += 1;
                let id = id_text(&o.operator_id);
                if !id.is_empty() {
                    facts.operator_id = Some(id);
                }
            }
        }
    });
}

/// Bounds one track's stored path: at most `max` evenly spaced samples, first
/// and last always kept.
fn decimate(path: &mut Vec<[f64; 2]>, max: usize) {
    if path.len() <= max || max < 2 {
        return;
    }
    let step = (path.len() - 1) as f64 / (max - 1) as f64;
    let kept: Vec<[f64; 2]> = (0..max).map(|i| path[(i as f64 * step).round() as usize]).collect();
    *path = kept;
}

/// Analyze a captured Remote ID file (pcap, pcapng or RIDF) with the real
/// crates and return the full field assessment as JSON. `{"ok":false,...}`
/// when the bytes are not a capture or carry no Remote ID.
#[wasm_bindgen]
pub fn analyze_capture(bytes: &[u8]) -> String {
    let frames = match capture::frames(bytes) {
        Ok(f) => f,
        Err(e) => {
            let code = if matches!(e, capture::CaptureError::UnknownFormat) {
                "unknown_format"
            } else {
                "corrupt"
            };
            return capture_error(code, format!("{e:?}"));
        }
    };
    if frames.is_empty() {
        return capture_error(
            "no_remote_id",
            "The file parses as a capture but contains no decodable Remote ID broadcasts.".into(),
        );
    }

    let now = frames.iter().map(|f| f.time_us / 1000).max().unwrap_or(0) + 1;
    let t0 = frames.iter().map(|f| f.time_us / 1000).min().unwrap_or(0);
    let utc_clock = frames.first().map(|f| f.utc_clock).unwrap_or(false);

    // Decode every frame for the report facts, and feed the same observations
    // to the sentinel for the verdict.
    let mut facts: HashMap<Transmitter, TrackFacts> = HashMap::new();
    let mut hist: HashMap<&'static str, usize> = HashMap::new();
    let mut identities = BTreeSet::new();
    let mut transports: HashMap<&'static str, usize> = HashMap::new();
    for f in &frames {
        *transports.entry(f.transport.name()).or_insert(0) += 1;
        fold_frame(f, facts.entry(f.transmitter).or_default(), &mut hist, &mut identities);
    }

    let observations = frames.iter().map(|f| (f.transmitter, f.observation(true)));
    let field = FieldAssessment::from_observations(observations, now);

    let genuine_tx = field.most_likely_real.as_ref().map(|t| t.transmitter);
    let to_track = |t: &TrackScore| -> CaptureTrackJson {
        let mut d = facts.get(&t.transmitter).cloned().unwrap_or_default();
        decimate(&mut d.path, 400);
        let mut uas_ids: Vec<String> = d.uas_ids.into_iter().collect();
        uas_ids.truncate(64);
        CaptureTrackJson {
            label: transmitter_label(&t.transmitter),
            uas_id: t.uas_id.clone(),
            uas_ids,
            lat: d.lat,
            lon: d.lon,
            rssi_dbm: d.rssi,
            verdict: t.per_unit.verdict.name().to_string(),
            reasons: t.per_unit.reasons.iter().map(|r| r.name().to_string()).collect(),
            auth: t.per_unit.auth.name().to_string(),
            signatures: t.fake_signals.iter().map(|s| s.name().to_string()).collect(),
            real_score: t.real_score,
            is_genuine: Some(t.transmitter) == genuine_tx,
            frames: d.frames,
            first_ms: d.first_ms,
            last_ms: d.last_ms,
            operator_lat: d.operator_lat,
            operator_lon: d.operator_lon,
            operator_id: d.operator_id,
            self_id: d.self_id,
            path: d.path,
        }
    };

    let mut tracks: Vec<CaptureTrackJson> = field.tracks.iter().map(to_track).collect();
    // The report leads with the likely-genuine track, then the most anomalous.
    tracks.sort_by(|a, b| {
        b.is_genuine
            .cmp(&a.is_genuine)
            .then(b.real_score.total_cmp(&a.real_score))
    });

    let mut sig_hist: HashMap<&'static str, usize> = HashMap::new();
    for t in &field.tracks {
        for s in t.fake_signals.iter() {
            *sig_hist.entry(s.name()).or_insert(0) += 1;
        }
    }
    let mut signature_histogram: Vec<(String, usize)> =
        sig_hist.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    signature_histogram.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let genuine = field.most_likely_real.as_ref().map(|t| {
        let c = to_track(t);
        GenuineJson {
            label: c.label,
            uas_id: c.uas_id,
            lat: c.lat,
            lon: c.lon,
            rssi_dbm: c.rssi_dbm,
            real_score: c.real_score,
            verdict: c.verdict,
            reasons: t.genuine_reasons.clone(),
            path: c.path,
        }
    });

    let mut messages: Vec<(String, usize)> = hist.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    messages.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut transports: Vec<(String, usize)> = transports.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    transports.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let out = CaptureJson {
        frames: frames.len(),
        transmitters: facts.len(),
        identities: identities.len(),
        window_ms: now - 1 - t0,
        utc_clock,
        transports,
        messages,
        flood: FloodJson {
            level: field.flood.level.name().to_string(),
            confidence: field.flood.confidence,
        },
        signature_histogram,
        genuine,
        tracks,
    };

    serde_json::to_string(&out).unwrap_or_else(|e| capture_error("internal", e.to_string()))
}
