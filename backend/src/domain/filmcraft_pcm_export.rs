//! Source-project PCM export, preserving Filmcraft integer quantisation and container bytes.
//! MIT reference formulas: docs/licenses/filmcraft-MIT.txt.
use super::{filmcraft_project_audio::SourceProjectAudioRuntime,filmcraft_wav_audio::SourceWavPool};
use anyhow::{ensure, Context, Result};
#[derive(Clone, Copy, Debug)]
pub enum PcmFormat { Wav, Aiff }
/// Export at the authored root sequence sample rate. Output supports mono/stereo/5.1 16/24-bit.
pub fn export_pcm(
    runtime:&mut SourceProjectAudioRuntime, sources:&SourceWavPool, sequence:u64,
    start:i64, frames:usize, channels:u16, bits:u16, format:PcmFormat,
) -> Result<Vec<u8>> {
    export_pcm_with_reader(runtime,sequence,start,frames,channels,bits,format,|id,start,frames,rate|sources.read(id,start,frames,rate))
}
pub fn export_pcm_with_reader(
    runtime:&mut SourceProjectAudioRuntime, sequence:u64, start:i64, frames:usize,
    channels:u16, bits:u16, format:PcmFormat,
    files:impl FnMut(u64,i64,usize,u32)->Result<Option<Vec<Vec<f32>>>>,
) -> Result<Vec<u8>> {
    ensure!(matches!(channels,1|2|6) && matches!(bits,16|24), "unsupported PCM output format");
    ensure!(frames <= 4_194_304, "PCM export range exceeds render budget");
    let rate=runtime.document().project().get("items").and_then(|v|v.get(sequence.to_string()))
        .and_then(|v|v.pointer("/kind/Sequence/settings/sample_rate")).and_then(serde_json::Value::as_u64).context("missing export sample rate")?;
    let rate=u32::try_from(rate)?;
    ensure!((8000..=192000).contains(&rate), "unsupported export sample rate");
    let bus=runtime.render_media_channels(sequence,start,frames,channels as usize,files)?;
    ensure!(bus.len()==channels as usize && bus.iter().all(|c|c.len()==frames), "invalid export stereo bus");
    let mut inter=Vec::with_capacity(frames*channels as usize);
    for i in 0..frames {
        for channel in &bus {inter.push(channel[i]);}
    }
    ensure!(inter.iter().all(|s|s.is_finite()), "nonfinite PCM export");
    Ok(match format {PcmFormat::Wav=>write_wav(&inter,channels,rate,bits),PcmFormat::Aiff=>write_aiff(&inter,channels,rate,bits)})
}
/// Create a new host-selected output file. Failed writes remove this call's partial file.
pub fn save_new(path:&std::path::Path, bytes:&[u8]) -> Result<()> {
    use std::io::Write;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    let result=(|| {file.write_all(bytes)?; file.sync_all()?; Ok::<_,std::io::Error>(())})();
    drop(file);
    if result.is_err() {let _=std::fs::remove_file(path);}
    result?; Ok(())
}
/// Quantise one sample to a signed integer of `bits` (16 or 24).
pub(crate) fn quantise(s: f32, bits: u16) -> i32 {
    let max = if bits >= 24 { 8_388_607.0 } else { 32_767.0 };
    (s.clamp(-1.0, 1.0) * max).round() as i32
}

/// A RIFF/WAVE file of interleaved samples. More than two channels are written as
/// `WAVE_FORMAT_EXTENSIBLE` with the speaker mask of the layout (6 channels: L, R, C, LFE, Ls, Rs
/// = `0x3F`).
fn write_wav(interleaved: &[f32], channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let bits: u16 = if bits >= 24 { 24 } else { 16 };
    let bps = (bits / 8) as u32;
    let data_len = interleaved.len() as u32 * bps;
    let ext = channels > 2;
    let fmt_len: u32 = if ext { 40 } else { 16 };
    let mut v = Vec::with_capacity(28 + fmt_len as usize + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(20 + fmt_len + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&fmt_len.to_le_bytes());
    v.extend_from_slice(&(if ext { 0xFFFEu16 } else { 1 }).to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&sample_rate.to_le_bytes());
    v.extend_from_slice(&(sample_rate * channels as u32 * bps).to_le_bytes());
    v.extend_from_slice(&(channels * bps as u16).to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    if ext {
        // cbSize, valid bits, channel mask, KSDATAFORMAT_SUBTYPE_PCM
        v.extend_from_slice(&22u16.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        let mask: u32 = if channels == 6 { 0x3F } else { 0 };
        v.extend_from_slice(&mask.to_le_bytes());
        v.extend_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);
    }
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for &s in interleaved {
        let q = quantise(s, bits).to_le_bytes();
        v.extend_from_slice(&q[..bps as usize]);
    }
    v
}

/// IEEE 754 80-bit extended encoding of a sample rate (AIFF `COMM`).
fn extended(rate: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate == 0 {
        return out;
    }
    let shift = rate.leading_zeros();
    let mant = (rate as u64) << (32 + shift);
    let exp = 16383 + 31 - shift as u16;
    out[..2].copy_from_slice(&exp.to_be_bytes());
    out[2..].copy_from_slice(&mant.to_be_bytes());
    out
}

/// An AIFF file (`FORM`/`AIFF`, `COMM` + `SSND`) of interleaved samples.
fn write_aiff(interleaved: &[f32], channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let bits: u16 = if bits >= 24 { 24 } else { 16 };
    let bps = (bits / 8) as usize;
    let frames = interleaved.len() / channels.max(1) as usize;
    let data_len = (interleaved.len() * bps) as u32;
    let ssnd_len = 8 + data_len;
    let form_len = 4 + (8 + 18) + (8 + ssnd_len) + (ssnd_len & 1);
    let mut v = Vec::with_capacity(form_len as usize + 8);
    v.extend_from_slice(b"FORM");
    v.extend_from_slice(&form_len.to_be_bytes());
    v.extend_from_slice(b"AIFFCOMM");
    v.extend_from_slice(&18u32.to_be_bytes());
    v.extend_from_slice(&channels.to_be_bytes());
    v.extend_from_slice(&(frames as u32).to_be_bytes());
    v.extend_from_slice(&bits.to_be_bytes());
    v.extend_from_slice(&extended(sample_rate));
    v.extend_from_slice(b"SSND");
    v.extend_from_slice(&ssnd_len.to_be_bytes());
    v.extend_from_slice(&0u32.to_be_bytes()); // offset
    v.extend_from_slice(&0u32.to_be_bytes()); // block size
    for &s in interleaved {
        let q = quantise(s, bits).to_be_bytes();
        v.extend_from_slice(&q[4 - bps..]);
    }
    if ssnd_len & 1 == 1 {
        v.push(0);
    }
    v
}

/// Checked original RIFF/WAVE or AIFF header; payload is streamed separately.
pub fn header(frames:u64,rate:u32,channels:u16,bits:u16,format:PcmFormat)->Result<Vec<u8>> {
    ensure!(frames>0&&matches!(channels,1|2|6)&&matches!(bits,16|24)&&(8000..=192000).contains(&rate),"unsupported PCM output format");
    let data=frames.checked_mul(u64::from(channels)).and_then(|n|n.checked_mul(u64::from(bits/8))).context("PCM size overflow")?;
    let data=u32::try_from(data).context("PCM container exceeds 32-bit size")?;
    match format {
        PcmFormat::Wav=>{let mut header=write_wav(&[],channels,rate,bits);let size=data.checked_add(u32::try_from(header.len()-8)?).context("WAV container size overflow")?;header[4..8].copy_from_slice(&size.to_le_bytes());let at=header.len()-4;header[at..].copy_from_slice(&data.to_le_bytes());Ok(header)}
        PcmFormat::Aiff=>{let mut header=write_aiff(&[],channels,rate,bits);let size=data.checked_add(46).and_then(|n|n.checked_add(data&1)).context("AIFF container size overflow")?;header[4..8].copy_from_slice(&size.to_be_bytes());header[22..26].copy_from_slice(&u32::try_from(frames)?.to_be_bytes());header[42..46].copy_from_slice(&data.checked_add(8).context("AIFF sound size overflow")?.to_be_bytes());Ok(header)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::filmcraft_project_file::SourceProjectDocument;
    #[test]
    fn original_project_pcm_export_bytes() {
        let project:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wav-project.json")).unwrap();
        let expected:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-pcm-export.json")).unwrap();
        assert_eq!(expected["commit"],project["commit"]);
        let bytes:Vec<u8>=project["wav"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
        let mut count=0;
        for (ci,case) in project["cases"].as_array().unwrap().iter().enumerate() {
            let mut sources=SourceWavPool::default();sources.bind(case["source"].as_u64().unwrap(),bytes.clone().into()).unwrap();
            for channels in [1u16,2] {for bits in [16u16,24] {for (format,key) in [(PcmFormat::Wav,"wav"),(PcmFormat::Aiff,"aiff")] {
                let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
                let mut runtime=SourceProjectAudioRuntime::new(document).unwrap();
                for (ri,request) in case["requests"].as_array().unwrap().iter().enumerate() {
                    let output=export_pcm(&mut runtime,&sources,case["root"].as_u64().unwrap(),request["start"].as_i64().unwrap(),request["frames"].as_u64().unwrap() as usize,channels,bits,format).unwrap();
                    let oi=((channels-1)*2+(bits==24) as u16)as usize;
                    let reference:Vec<u8>=expected["cases"][ci][ri][oi][key].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
                    assert_eq!(output,reference,"PCM export {ci}/{ri}/{channels}/{bits}/{key}");count+=1;
                }
                assert_eq!(serde_json::from_slice::<serde_json::Value>(&runtime.document().encode_preserved().unwrap()).unwrap(),case["document"]);
            }}}
        }
        assert_eq!(count,768);
    }
    #[test]
    fn pcm_new_file_preserves_existing_output() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("audio.wav");
        save_new(&path,b"first").unwrap();assert!(save_new(&path,b"second").is_err());
        assert_eq!(std::fs::read(path).unwrap(),b"first");
    }
}
