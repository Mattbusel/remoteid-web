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

use remoteid::{Message, Observation, Transmitter, Transport};
use remoteid_scenario::{generate, EmittedFrame, Scenario, ScenarioKind};
use remoteid_sentinel::{FieldAssessment, TrackScore};
use serde::Serialize;
use std::collections::HashMap;
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
