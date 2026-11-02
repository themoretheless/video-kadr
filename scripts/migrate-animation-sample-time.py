#!/usr/bin/env python3
"""Audit or apply explicit animation time propagation. Default is read-only."""
from pathlib import Path
import argparse
import re

parser = argparse.ArgumentParser()
parser.add_argument('--apply', action='store_true')
parser.add_argument('--domain-dir', type=Path, help='Explicit isolated domain snapshot for validation')
args = parser.parse_args()
root = args.domain_dir or Path(__file__).resolve().parents[1] / 'backend/src/domain'
changes = {}
count = 0
for path in sorted(root.glob('*.rs')):
    if path.name in {'animation_sample_time.rs', 'keyframe_source_time.rs'}:
        continue
    source = path.read_text()
    edits = []
    for match in re.finditer(r'pub(?:\(crate\))?\s+fn\s+(sample_\w+|sample)\s*\(', source):
        if match.group(1) == 'sample' and path.name not in {'lightning_animation.rs', 'write_on_animation.rs', 'warp_stabilizer_animation.rs'}:
            continue
        opening = match.end() - 1
        depth = 1
        end = opening + 1
        while depth and end < len(source):
            depth += (source[end] == '(') - (source[end] == ')')
            end += 1
        signature = source[opening + 1:end - 1]
        clock = re.search(r'\b(time|t|local|local_us|time_us|local_tick)\s*:\s*u64\b', signature)
        if not clock:
            continue
        name = clock.group(1)
        start = opening + 1 + clock.start()
        stop = opening + 1 + clock.end()
        body = source.index('{', end)
        edits.extend([(start, stop, name + ': impl Into<super::animation_sample_time::AnimationSampleTime>'),
                      (body + 1, body + 1, '\n    let ' + name + ': super::animation_sample_time::AnimationSampleTime = ' + name + '.into();\n')])
        count += 1
        print(path.name + ':' + match.group(1) + ':' + name)
    for start, stop, value in sorted(edits, reverse=True):
        source = source[:start] + value + source[stop:]
    if path.name == 'nested_composition.rs':
        old = 'track.sample_seconds(local_us as f64 / 1_000_000.0)?'
        assert source.count(old) == 1
        source = source.replace(old, 'local_us.sample_track(track)?')
    if path.name in {'graphic_text_animation.rs', 'graphic_shape_animation.rs'}:
        result = 'GraphicTextParameters' if path.name == 'graphic_text_animation.rs' else 'GraphicShape'
        animation = 'GraphicTextAnimation' if path.name == 'graphic_text_animation.rs' else 'GraphicShapeAnimation'
        old = '    local: u64,'
        assert source.count(old) == 1
        source = source.replace(old, '    local: impl Into<super::animation_sample_time::AnimationSampleTime>,', 1)
        old = '    sample_signed(parameters, animation, i128::from(local), canvas)'
        assert source.count(old) == 1
        source = source.replace(old, '    let local = local.into();\n    sample_with(parameters, animation, canvas, |track| local.sample_track(track))', 1)
        old = 'pub fn sample_seconds(\n    mut parameters: ' + result
        assert source.count(old) == 1
        source = source.replace(old, 'pub fn sample_seconds(\n    parameters: ' + result, 1)
        marker = ') -> Result<' + result + '> {\n    parameters.validate()?;'
        assert source.count(marker) == 1
        replacement = (') -> Result<' + result + '> {\n'
            '    sample_with(parameters, animation, canvas, |track| track.sample_seconds(seconds))\n}\n'
            'fn sample_with(\n    mut parameters: ' + result + ',\n'
            '    animation: &' + animation + ',\n    canvas: (u32, u32),\n'
            '    sample_track: impl Fn(&super::keyframes::KeyframeTrack<f64>) -> Result<f64, super::keyframes::KeyframeError>,\n'
            ') -> Result<' + result + '> {\n    parameters.validate()?;')
        source = source.replace(marker, replacement, 1)
        assert source.count('track.sample_seconds(seconds)?') == 1
        source = source.replace('track.sample_seconds(seconds)?', 'sample_track(track)?', 1)
        count += 1
    if source != path.read_text():
        changes[path] = source
assert count == 118, f'Unexpected sampling inventory: {count}; review before applying'
print(f'{count} functions across {len(changes)} files; apply={args.apply}')
if args.apply:
    for path, source in changes.items():
        path.write_text(source)
