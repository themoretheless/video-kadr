//! Native Filmcraft WAV audio reader. Sample conversion follows the MIT original.
use anyhow::{ensure, Context, Result};
use std::{collections::BTreeMap, sync::Arc};
pub struct WavSource {
    bytes: Arc<[u8]>, data_off: usize, data_len: usize,
    channels: usize, bits: u16, float: bool, rate: u32,
    pub time_reference: Option<u64>,
}
fn le16(b: &[u8], o: usize) -> u16 { u16::from_le_bytes(b[o..o+2].try_into().unwrap()) }
fn le32(b: &[u8], o: usize) -> u32 { u32::from_le_bytes(b[o..o+4].try_into().unwrap()) }
impl WavSource {
    pub fn parse(bytes: Arc<[u8]>) -> Result<Self> {
        let b = &bytes[..];
        ensure!(b.len() >= 12 && matches!(&b[..4], b"RIFF" | b"RF64") && &b[8..12] == b"WAVE", "invalid WAV header");
        let mut pos = 12;
        let (mut fmt, mut data, mut bext, mut ixml) = (None, None, None, None);
        while pos + 8 <= b.len() {
            let id = &b[pos..pos+4];
            let mut len = le32(b,pos+4) as usize;
            let body = pos+8;
            if id == b"data" && (len == u32::MAX as usize || len > b.len()-body) { len = b.len()-body; }
            if len > b.len()-body { break; }
            match id {
                b"fmt " if len >= 16 => {
                    let mut tag = le16(b,body);
                    if tag == 0xFFFE && len >= 40 { tag = le16(b,body+24); }
                    fmt = Some((tag,le16(b,body+2),le32(b,body+4),le16(b,body+14)));
                }
                b"data" => data = Some((body,len)),
                b"bext" if len >= 346 => bext = Some(le32(b,body+338) as u64 | (le32(b,body+342) as u64)<<32),
                b"iXML" => ixml = ixml_time_reference(&b[body..body+len]),
                _ => {}
            }
            pos = body+len+(len&1);
        }
        let (tag,ch,rate,bits) = fmt.context("missing WAV format")?;
        let (data_off,len) = data.context("missing WAV samples")?;
        ensure!((tag == 1 && matches!(bits,8|16|24|32)) || (tag == 3 && matches!(bits,32|64)), "unsupported WAV encoding");
        ensure!((1..=16).contains(&ch) && rate > 0, "invalid WAV channels/rate");
        let frame_bytes = ch as usize * bits as usize/8;
        Ok(Self { bytes, data_off, data_len:len/frame_bytes*frame_bytes, channels:ch as usize,bits,float:tag==3,rate,time_reference:bext.or(ixml) })
    }
    pub fn sample_rate(&self) -> u32 { self.rate }
    pub fn channels(&self) -> usize { self.channels }
    pub fn frames(&self) -> usize { self.data_len/(self.channels*self.bits as usize/8) }
    pub fn audio(&self, start:i64, frames:usize, rate:u32) -> Result<Vec<Vec<f32>>> {
        ensure!(rate > 0 && frames.checked_mul(self.channels).is_some_and(|n|n<=8_388_608), "WAV request exceeds budget");
        ensure!(start.checked_add(i64::try_from(frames)?).is_some(), "WAV sample range overflow");
        let total=self.frames();
        let mut out=vec![vec![0.;frames];self.channels];
        let ratio=self.rate as f64/rate as f64;
        for i in 0..frames {
            let src=(start+i as i64) as f64*ratio;
            if src<0. {continue;}
            let i0=src.floor() as usize;
            if i0>=total {break;}
            let fr=(src-i0 as f64) as f32;
            for (c,channel) in out.iter_mut().enumerate() {
                let a=self.sample(i0,c);
                let b=if i0+1<total {self.sample(i0+1,c)} else {a};
                channel[i]=a+(b-a)*fr;
                ensure!(channel[i].is_finite(), "nonfinite WAV audio");
            }
        }
        Ok(out)
    }
    fn sample(&self, frame: usize, ch: usize) -> f32 {
        let bps = self.bits as usize / 8;
        let o = self.data_off + (frame * self.channels + ch) * bps;
        let b = &self.bytes;
        match (self.bits, self.float) {
            (8, _) => (b[o] as f32 - 128.0) / 128.0,
            (16, _) => i16::from_le_bytes([b[o], b[o + 1]]) as f32 / 32768.0,
            (24, _) => ((i32::from_le_bytes([0, b[o], b[o + 1], b[o + 2]]) >> 8) as f32) / 8_388_608.0,
            (32, false) => i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as f32 / 2_147_483_648.0,
            (32, true) => f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]),
            (64, true) => f64::from_le_bytes(b[o..o + 8].try_into().unwrap_or([0; 8])) as f32,
            _ => 0.0,
        }
    }
}
fn ixml_time_reference(x: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(x);
    let field = |tag: &str| -> Option<u64> {
        let open = format!("<{tag}>");
        let a = text.find(&open)? + open.len();
        let b = a + text[a..].find('<')?;
        text[a..b].trim().parse().ok()
    };
    let low = field("BWF_TIME_REFERENCE_LOW")?;
    Some(low | field("BWF_TIME_REFERENCE_HIGH").unwrap_or(0) << 32)
}

/// Encode interleaved f32 samples as a 16-bit PCM WAV file.
pub fn write_wav16(samples: &[f32], channels: u16, rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut v = Vec::with_capacity(44 + data_len);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    v.extend_from_slice(&(channels * 2).to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        v.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    v
}

/// Explicit root-media bindings supplied by the host. Loading bytes never follows project paths.
#[derive(Default)]
pub struct SourceWavPool { sources: BTreeMap<u64, WavSource> }
impl SourceWavPool {
    pub fn bind(&mut self, root_media:u64, bytes:Arc<[u8]>) -> Result<()> {
        let source=WavSource::parse(bytes)?;
        self.sources.insert(root_media,source); Ok(())
    }
    pub fn bind_file(&mut self, root_media:u64, path:&std::path::Path, byte_budget:u64) -> Result<()> {
        use std::io::Read;
        ensure!(byte_budget > 0 && byte_budget <= 1_073_741_824, "invalid WAV file budget");
        let file=std::fs::File::open(path)?;
        ensure!(file.metadata()?.len() <= byte_budget, "WAV file exceeds byte budget");
        let mut bytes=vec![];
        file.take(byte_budget+1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= byte_budget, "WAV file grew beyond byte budget");
        self.bind(root_media,bytes.into())
    }
    pub fn read(&self,id:u64,start:i64,frames:usize,rate:u32) -> Result<Option<Vec<Vec<f32>>>> {
        self.sources.get(&id).map(|source|source.audio(start,frames,rate)).transpose()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusted_wav_file_binding_and_failed_replacement() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wav-audio.json")).unwrap();
        let bytes:Vec<u8>=fixture["cases"][0]["bytes"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("audio.wav");std::fs::write(&path,&bytes).unwrap();
        let mut pool=SourceWavPool::default();
        assert!(pool.bind_file(7,&path,bytes.len() as u64-1).is_err());
        assert!(pool.read(7,0,3,44100).unwrap().is_none());
        pool.bind_file(7,&path,bytes.len() as u64).unwrap();
        let before=pool.read(7,0,31,44100).unwrap().unwrap();
        assert!(pool.bind(7,Arc::from(&b"bad"[..])).is_err());
        assert_eq!(before,pool.read(7,0,31,44100).unwrap().unwrap());
        assert!(pool.read(7,i64::MAX,1,44100).is_err());
    }
    #[test]
    fn original_wav_decode_and_resample_bits() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wav-audio.json")).unwrap();
        assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let mut checked=0;
        for case in fixture["cases"].as_array().unwrap() {
            let bytes:Vec<u8>=case["bytes"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
            let source=WavSource::parse(bytes.into()).unwrap();
            assert_eq!(serde_json::json!(source.time_reference),case["time_reference"]);
            for request in case["requests"].as_array().unwrap() {
                let actual=source.audio(request["start"].as_i64().unwrap(),31,request["rate"].as_u64().unwrap() as u32).unwrap();
                let bits:Vec<Vec<u32>>=actual.iter().map(|ch|ch.iter().map(|v|v.to_bits()).collect()).collect();
                checked+=bits.iter().map(Vec::len).sum::<usize>();
                assert_eq!(serde_json::json!(bits),request["bits"]);
            }
        }
        assert_eq!(checked,fixture["samples"].as_u64().unwrap() as usize);
        assert!(WavSource::parse(Arc::from(&b"invalid"[..])).is_err());
    }
}
