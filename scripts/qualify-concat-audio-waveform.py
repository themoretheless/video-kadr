#!/usr/bin/env python3
"""Build a pre-AAC PCM observation adapter for pinned Concat's actual mixer.

Original media source, decoder, graph, feeding, sink clock and AAC encoder stay
unchanged. One PCM observer is inserted immediately before send_frame; an
optional graph.dump observer after validation reports negotiated formats.
All copied source/build products live in ignored backend/target. The reference
checkout itself is never modified. Run the binary with plan.json out.m4a tap.f32
(or '-' instead of the tap) and compare encoded audio to test observer neutrality.
"""
from pathlib import Path
import hashlib
import json
import subprocess
root = Path(__file__).resolve().parent.parent
ref = root / 'backend/target/concat-reference/src/crates'
out = root / 'backend/target/concat-audio-waveform-oracle'
out.mkdir(exist_ok=True)
def item(text, signature):
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
audio = (ref / 'concat-media/src/audio.rs').read_text()
needle = '                    encoder\n                        .send_frame(&mixed)'
assert audio.count(needle) == 1
instrumented = audio.replace(needle, '                    crate::capture_pcm(&mixed);\n' + needle)
dump_needle = '    // The encoder, and the file it writes into.'
assert instrumented.count(dump_needle) == 1
instrumented = instrumented.replace(dump_needle, '    if std::env::var_os("CONCAT_ORACLE_GRAPH_DUMP").is_some() { eprintln!("{}", graph.dump()); }\n\n' + dump_needle)
(out / 'observed_audio.rs').write_text(instrumented)
probe = (ref / 'concat-media/src/probe.rs').read_text()
(out / 'probe_adapter.rs').write_text('use ffmpeg_the_third as ffmpeg;\n' + item(probe, 'fn is_audio(') + '\n' + item(probe, 'pub(crate) fn audio_stream_index('))
main = '\n'.join([
    f'#[path="{ref / "concat-media/src/error.rs"}"] mod error;',
    f'#[path="{ref / "concat-media/src/ffi.rs"}"] mod ffi;',
    '#[path="probe_adapter.rs"] mod probe;',
    '#[path="observed_audio.rs"] mod audio;',
    f'include!("{root / "scripts/reference/concat_audio_waveform.rs"}");',
])
(out / 'main.rs').write_text(main)
(out / 'Cargo.toml').write_text(f'''[package]
name = "video-kadr-concat-audio-waveform"
version = "0.1.0"
edition = "2024"
[workspace]
[[bin]]
name = "concat-audio-waveform"
path = "main.rs"
[dependencies]
concat-core = {{ path = "{ref / 'concat-core'}" }}
ffmpeg-the-third = "6"
libc = "0.2"
log = "0.4"
thiserror = "2"
serde_json = "1"
''')
(out / 'provenance.json').write_text(json.dumps({
    'source': str(ref / 'concat-media/src/audio.rs'),
    'originalSha256': hashlib.sha256(audio.encode()).hexdigest(),
    'observedSha256': hashlib.sha256(instrumented.encode()).hexdigest(),
    'observer': 'capture_pcm(&mixed) immediately before the single encoder.send_frame(&mixed)',
    'modifiedSourceCalls': 2,
    'optionalGraphObserver': 'graph.dump() after validation when CONCAT_ORACLE_GRAPH_DUMP is set',
}, indent=2) + '\n')
subprocess.run(['cargo', '+1.97.1', 'build', '--manifest-path', str(out / 'Cargo.toml'), '--target-dir', str(root / 'backend/target/concat-audio-waveform-build')], check=True)
print(root / 'backend/target/concat-audio-waveform-build/debug/concat-audio-waveform')
