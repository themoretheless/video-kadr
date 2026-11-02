//! Sweet compand/deesser stages on negotiated double planar packets.
use super::{packet_echo::floating64, packet_pitch::Block};
use crate::domain::{concat_compand::Compand, concat_deesser::Deesser, concat_format_plan::Format};
use anyhow::{ensure, Result};
enum Core {
    Compand(Compand),
    Deesser(Deesser),
}
pub(crate) struct Stage {
    core: Core,
    channels: usize,
    failed: bool,
}
impl Stage {
    fn shape(format: Format, rate: u32, channels: usize) -> Result<()> {
        ensure!(
            format == Format::F64P
                && (8000..=192000).contains(&rate)
                && (1..=16).contains(&channels),
            "dynamics requires negotiated planar double and valid audio shape"
        );
        Ok(())
    }
    pub(crate) fn compand(format: Format, rate: u32, channels: usize) -> Result<Self> {
        Self::shape(format, rate, channels)?;
        Ok(Self {
            core: Core::Compand(Compand::sweet(rate, channels)),
            channels,
            failed: false,
        })
    }
    pub(crate) fn deesser(
        format: Format,
        rate: u32,
        channels: usize,
        intensity: f64,
        maximum: f64,
        frequency: f64,
    ) -> Result<Self> {
        Self::shape(format, rate, channels)?;
        ensure!(
            [intensity, maximum, frequency]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid deesser parameters"
        );
        Ok(Self {
            core: Core::Deesser(Deesser::new(rate, channels, intensity, maximum, frequency)),
            channels,
            failed: false,
        })
    }
    pub(crate) fn reset(&mut self) {
        match &mut self.core {
            Core::Compand(c) => c.reset(),
            Core::Deesser(d) => d.reset(),
        };
        self.failed = false;
    }
    pub(crate) fn push_packet(&mut self, input: Block) -> Result<Block> {
        ensure!(
            !self.failed,
            "dynamics requires reset after processing failure"
        );
        let mut data = floating64(input);
        ensure!(data.len() == self.channels, "dynamics channel mismatch");
        let count = data[0].len();
        ensure!(
            count <= 1048576
                && data
                    .iter()
                    .all(|ch| ch.len() == count && ch.iter().all(|v| v.is_finite())),
            "invalid dynamics packet"
        );
        self.failed = true;
        let mut frame = [0.; 16];
        for i in 0..count {
            for ch in 0..self.channels {
                frame[ch] = data[ch][i];
            }
            let samples = &mut frame[..self.channels];
            match &mut self.core {
                Core::Compand(c) => c.process_frame(samples),
                Core::Deesser(d) => d.process_frame(samples),
            }
            ensure!(
                samples.iter().all(|v| v.is_finite()),
                "nonfinite dynamics output"
            );
            for ch in 0..self.channels {
                data[ch][i] = frame[ch];
            }
        }
        self.failed = false;
        Ok(Block::F64(data))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn original_sweet_dynamics_packets_and_reset_match_double_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.f64le");
        let mut compared = 0;
        for rate in [8000, 44100, 48000, 192000] {
            for channels in [1, 2] {
                let frames = rate as usize / 7 + 17;
                let input = (0..channels)
                    .map(|ch| {
                        (0..frames)
                            .map(|i| {
                                let h = (i as u32)
                                    .wrapping_add(ch as u32 * 1061)
                                    .wrapping_mul(1664525)
                                    .wrapping_add(1013904223);
                                if i % 701 == 0 {
                                    2.5
                                } else {
                                    (((h >> 8) % 8193) as i32 - 4096) as f64 / 4096.
                                        * if ch == 0 { 0.9 } else { 0.03 }
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let mut file = std::fs::File::create(&path).unwrap();
                for i in 0..frames {
                    for ch in &input {
                        file.write_all(&ch[i].to_le_bytes()).unwrap();
                    }
                }
                drop(file);
                for (compand, intensity) in [
                    (true, None),
                    (false, Some(0.18)),
                    (false, Some(0.388)),
                    (false, Some(0.5)),
                    (true, Some(0.18)),
                    (true, Some(0.388)),
                    (true, Some(0.5)),
                ] {
                    let mut filters = Vec::new();
                    if compand {
                        filters.push("compand=attacks=0.010:decays=0.180:points=-80/-80|-30/-24|-18/-13|-8/-5|0/-1.5:soft-knee=5:gain=1".to_owned());
                    }
                    if let Some(i) = intensity {
                        filters.push(format!("deesser=i={i}:m=0.45:f=0.55"));
                    }
                    let original = std::process::Command::new("ffmpeg")
                        .args([
                            "-nostdin",
                            "-v",
                            "error",
                            "-f",
                            "f64le",
                            "-ar",
                            &rate.to_string(),
                            "-ac",
                            &channels.to_string(),
                            "-i",
                        ])
                        .arg(&path)
                        .args([
                            "-af",
                            &filters.join(","),
                            "-c:a",
                            "pcm_f64le",
                            "-f",
                            "f64le",
                            "pipe:1",
                        ])
                        .output()
                        .unwrap();
                    assert!(
                        original.status.success(),
                        "{}",
                        String::from_utf8_lossy(&original.stderr)
                    );
                    assert_eq!(original.stdout.len(), frames * channels * 8);
                    let mut stages = Vec::new();
                    if compand {
                        stages.push(Stage::compand(Format::F64P, rate, channels).unwrap());
                    }
                    if let Some(i) = intensity {
                        stages.push(
                            Stage::deesser(Format::F64P, rate, channels, i, 0.45, 0.55).unwrap(),
                        );
                    }
                    for grid in [1, 255, 1024] {
                        for stage in &mut stages {
                            stage.reset();
                        }
                        let mut actual = vec![Vec::new(); channels];
                        for at in (0..frames).step_by(grid) {
                            let end = (at + grid).min(frames);
                            let mut block =
                                Block::F64(input.iter().map(|ch| ch[at..end].to_vec()).collect());
                            for stage in &mut stages {
                                block = stage.push_packet(block).unwrap();
                            }
                            assert_eq!(block.frames(), end - at);
                            for (dst, ch) in actual.iter_mut().zip(floating64(block)) {
                                dst.extend(ch);
                            }
                        }
                        for (i, bytes) in original.stdout.chunks_exact(8).enumerate() {
                            let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                            let value = actual[i % channels][i / channels];
                            assert!((value-expected).abs()<=2e-12,"rate{rate} channels{channels} compand{compand} intensity{intensity:?} grid{grid} value{i}: {value} vs {expected}");
                            compared += 1;
                        }
                    }
                }
            }
        }
        println!("Sweet packet dynamics: {compared} double values");
    }
}
