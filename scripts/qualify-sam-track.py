#!/usr/bin/env python3
"""Actual isolated SAM track CLI against masks from the pinned original Brush."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import threading


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(65536), b''):
            digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    for name in ('binary', 'encoder', 'decoder', 'runtime', 'oracle', 'workroot'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    assert all(value.is_absolute() for value in vars(args).values())
    root = Path(tempfile.mkdtemp(prefix='sam-track-', dir=args.workroot))
    pins = {name + 'Hash': sha(getattr(args, name)) for name in ('encoder', 'decoder', 'runtime')}
    checked = []
    for case in sorted(args.oracle.iterdir()):
        if not case.is_dir():
            continue
        oracle = json.loads((case / 'reference.json').read_text())
        assert oracle['frames'] == 4 and oracle['seedFrame'] == 1
        for mode in ('sequence', 'empty', 'changed', 'occupied', 'late-occupied', 'bad-seed', 'bad-source', 'out-of-range', 'unknown'):
            folder = root / (case.name + '-' + mode)
            folder.mkdir(mode=0o700)
            source, seed, output = (folder / name for name in ('input.rgba', 'seed.gray', 'output.masks'))
            shutil.copyfile(case / 'source.rgba', source)
            shutil.copyfile(case / 'seed.gray', seed)
            if mode == 'empty':
                seed.write_bytes(bytes(65536))
            request = dict(schemaVersion=9, encoder=str(args.encoder), decoder=str(args.decoder),
                           runtimeLibrary=str(args.runtime), input=str(source), seed=str(seed), output=str(output),
                           sourceSha256=sha(source), seedSha256=sha(seed), width=oracle['width'], height=oracle['height'],
                           frames=4, seedFrame=1)
            if mode == 'occupied':
                output.write_bytes(b'foreign output')
            elif mode == 'bad-seed':
                request['seedSha256'] = '0' * 64
            elif mode == 'bad-source':
                request['sourceSha256'] = '0' * 64
            elif mode == 'out-of-range':
                request['seedFrame'] = 4
            elif mode == 'unknown':
                request['url'] = 'https://invalid'
            request_file = folder / 'request.json'
            request_file.write_text(json.dumps(request))
            process = subprocess.Popen([str(args.binary), '--sam-track', str(request_file)],
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            timer = threading.Timer(90, process.kill)
            timer.daemon = True
            timer.start()
            lines, progress, changed = [], [], False
            try:
                for line in process.stderr:
                    lines.append(line)
                    try:
                        event = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if event.get('event') == 'progress':
                        progress.append(event['completed'])
                        assert event['total'] == 4
                        if mode == 'changed' and event['completed'] >= 1 and not changed:
                            source.write_bytes(b'changed private RGBA sequence')
                            changed = True
                        if mode == 'late-occupied' and event['completed'] >= 1 and not changed:
                            output.write_bytes(b'foreign output')
                            changed = True
                stdout = process.stdout.read()
                code = process.wait()
            except BaseException:
                if process.poll() is None:
                    process.kill()
                process.communicate()
                raise
            finally:
                timer.cancel()
            (folder / 'stdout.txt').write_text(stdout)
            (folder / 'stderr.txt').write_text(''.join(lines))
            assert progress == sorted(progress), (case.name, mode, progress)
            if mode in ('sequence', 'empty'):
                assert code == 0, (case.name, mode, ''.join(lines))
                result = json.loads(stdout)
                expected = (case / 'reference.masks').read_bytes() if mode == 'sequence' else bytes(4 * 65536)
                assert output.read_bytes() == expected, (case.name, mode, 'original pixels')
                assert result['schemaVersion'] == 9 and result['maskBytes'] == 4 * 65536
                assert (result['maskWidth'], result['maskHeight']) == (256, 256)
                for key in ('width', 'height', 'frames', 'seedFrame', 'sourceHash'):
                    assert result[key] == oracle[key], (case.name, mode, key)
                assert result['seedHash'] == sha(seed) and result['outputHash'] == sha(output)
                for key, value in pins.items():
                    assert result[key] == value
                for key in ('trackedFrames', 'forwardLostAt', 'backwardLostAt'):
                    expected_value = oracle[key] if mode == 'sequence' else dict(trackedFrames=1, forwardLostAt=2, backwardLostAt=0)[key]
                    assert result[key] == expected_value, (case.name, mode, key)
                assert progress[0] == 0 and progress[-1] == 4
                assert stat.S_IMODE(output.stat().st_mode) == 0o600
            else:
                assert code != 0, (case.name, mode)
                if mode in ('occupied', 'late-occupied'):
                    assert output.read_bytes() == b'foreign output'
                else:
                    assert not output.exists(), (case.name, mode, 'published on error')
                if mode in ('changed', 'late-occupied'):
                    assert changed
            assert not list(folder.glob('.sam-track-*')), (case.name, mode, 'staging leak')
            checked.append(case.name + ':' + mode)
    assert len(checked) == 27
    print(json.dumps(dict(passed=True, cases=checked, artifacts=str(root))))


if __name__ == '__main__':
    main()
