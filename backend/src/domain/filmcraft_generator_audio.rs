//! Filmcraft synthetic audio. Source formulas retained under MIT; see docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DemoScene {
    Aurora,
    OceanSunset,
    CityNight,
    Dunes,
    Plasma,
    Forest,
}

impl DemoScene {
    pub const ALL: [DemoScene; 6] = [DemoScene::OceanSunset, DemoScene::Aurora, DemoScene::CityNight, DemoScene::Dunes, DemoScene::Forest, DemoScene::Plasma];
    pub fn file_name(self) -> &'static str {
        match self {
            DemoScene::Aurora => "Aurora_Timelapse.mp4",
            DemoScene::OceanSunset => "Ocean_Sunset.mp4",
            DemoScene::CityNight => "City_Night_Drive.mp4",
            DemoScene::Dunes => "Desert_Dunes.mp4",
            DemoScene::Plasma => "Neon_Loop.mov",
            DemoScene::Forest => "Misty_Forest.mp4",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Generator {
    BarsAndTone,
    ColorMatte {
        color: [f32; 4],
    },
    BlackVideo,
    TransparentVideo,
    CountingLeader,
    Demo(DemoScene),
    /// Pure audio tone (Hz, dBFS).
    Tone {
        hz: f32,
        db: f32,
    },
}


impl Generator {
    pub fn audio(&self, duration_ticks: i64, start: i64, frames: usize, rate: u32) -> Result<Option<Vec<Vec<f32>>>> {
        ensure!(rate > 0 && frames <= 4_194_304, "invalid generator audio request");
        ensure!(start.checked_add(i64::try_from(frames)?).is_some(), "generator sample range overflow");
        if !matches!(self, Self::BarsAndTone | Self::CountingLeader | Self::Demo(_) | Self::Tone { .. }) { return Ok(None); }
        let total = (duration_ticks as i128 * rate as i128).div_euclid(254_016_000_000) as i64;
        let mut out = vec![vec![0.; frames]; 2];
        for i in 0..frames {
            let n = start + i as i64;
            if n < 0 || n >= total { continue; }
            let (l, r) = audio_sample(self, n as f32 / rate as f32, n);
            ensure!(l.is_finite() && r.is_finite(), "nonfinite generator audio");
            out[0][i] = l; out[1][i] = r;
        }
        Ok(Some(out))
    }
}
fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn hash(n: i64) -> f32 {
    let mut x = n as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

fn audio_sample(g: &Generator, t: f32, n: i64) -> (f32, f32) {
    match g {
        Generator::BarsAndTone => {
            let s = (TAU * 1000.0 * t).sin() * db(-20.0);
            (s, s)
        }
        Generator::Tone { hz, db: d } => {
            let s = (TAU * hz * t).sin() * db(*d);
            (s, s)
        }
        Generator::CountingLeader => {
            // 1 kHz beep for one frame at each second mark (the "2-pop" style beep).
            let frac = t.fract();
            let s = if frac < 1.0 / 24.0 { (TAU * 1000.0 * t).sin() * db(-20.0) } else { 0.0 };
            (s, s)
        }
        Generator::Demo(scene) => demo_audio(*scene, t, n),
        _ => (0.0, 0.0),
    }
}

fn demo_audio(scene: DemoScene, t: f32, n: i64) -> (f32, f32) {
    // A slow pad: chord tones with gentle detune + scene texture.
    let chord: &[f32] = match scene {
        DemoScene::OceanSunset => &[220.0, 277.18, 329.63, 440.0],
        DemoScene::Aurora => &[196.0, 246.94, 293.66, 369.99],
        DemoScene::CityNight => &[110.0, 164.81, 207.65, 246.94],
        DemoScene::Dunes => &[146.83, 220.0, 261.63, 329.63],
        DemoScene::Plasma => &[130.81, 196.0, 261.63, 311.13],
        DemoScene::Forest => &[174.61, 220.0, 261.63, 349.23],
    };
    let env = 0.6 + 0.4 * (TAU * t / 8.0).sin();
    let mut l = 0.0;
    let mut r = 0.0;
    for (i, f) in chord.iter().enumerate() {
        let ph = TAU * f * t;
        let trem = 0.8 + 0.2 * (TAU * (0.2 + i as f32 * 0.07) * t).sin();
        l += (ph * 1.002).sin() * trem;
        r += (ph * 0.998).sin() * trem;
    }
    let noise = hash(n)
        * match scene {
            DemoScene::OceanSunset => 0.25 * (0.5 + 0.5 * (TAU * t / 6.0).sin()).powi(2),
            DemoScene::Forest => 0.05,
            DemoScene::CityNight => 0.08,
            _ => 0.02,
        };
    // A soft kick in the city/plasma scenes to make waveforms interesting.
    let beat = if matches!(scene, DemoScene::CityNight | DemoScene::Plasma) {
        let bt = (t * 2.0).fract() / 2.0;
        (TAU * 55.0 * bt).sin() * (-bt * 18.0).exp() * 0.9
    } else {
        0.0
    };
    let g = 0.09 * env;
    (l * g + noise + beat, r * g + noise * 0.9 + beat)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_generator_pcm_bits() {
        let gs = [Generator::BarsAndTone, Generator::CountingLeader, Generator::Tone {hz:437.5,db:-13.25}, Generator::Demo(DemoScene::Aurora),Generator::Demo(DemoScene::OceanSunset),Generator::Demo(DemoScene::CityNight),Generator::Demo(DemoScene::Dunes),Generator::Demo(DemoScene::Plasma),Generator::Demo(DemoScene::Forest)];
        let mut count = 0;
        for line in include_str!("../../tests/fixtures/filmcraft-generator-audio.txt").lines() {
            let v: Vec<i64> = line.split_whitespace().map(|s|s.parse().unwrap()).collect();
            let out = gs[v[0] as usize].audio(508_032_000_000,v[2],1,v[1] as u32).unwrap().unwrap();
            assert_eq!(out[0][0].to_bits(),v[3] as u32,"{line}");
            assert_eq!(out[1][0].to_bits(),v[4] as u32,"{line}"); count+=2;
        }
        assert_eq!(count,31_428);
        assert!(Generator::BlackVideo.audio(1,0,0,48000).unwrap().is_none());
        assert!(Generator::BarsAndTone.audio(1,i64::MAX,1,48000).is_err());
    }
}
