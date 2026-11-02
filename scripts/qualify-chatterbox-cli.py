#!/usr/bin/env python3
"""Exercise the real Rust CLI on an explicitly supplied public reference/model.
No model download, route mocks or private media. Keep owned qualification outputs.
"""
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import tempfile
import time
import wave

repo = Path(__file__).resolve().parents[1]
binary = Path(os.environ['VIDEO_KADR_TEST_CHATTERBOX_BIN']).resolve()
model = Path(os.environ['VIDEO_KADR_TEST_CHATTERBOX_MODEL']).resolve()
source = Path(os.environ['VIDEO_KADR_TEST_CHATTERBOX_REFERENCE']).resolve()
root = Path(tempfile.mkdtemp(prefix='chatterbox-cli-', dir=repo / 'backend/target/qualification-tts'))
reference = root / 'reference.wav'
with wave.open(str(source), 'rb') as reader:
    assert (reader.getnchannels(), reader.getsampwidth(), reader.getframerate()) == (1, 2, 24000)
    pcm = reader.readframes(144000)
assert len(pcm) == 288000
with wave.open(str(reference), 'wb') as writer:
    writer.setparams((1, 2, 24000, 0, 'NONE', 'not compressed'))
    writer.writeframes(pcm)
reference.chmod(0o600)

def request(name, text='Welcome to our video project.'):
    value = dict(schemaVersion=1, modelDir=str(model), reference=str(reference), output=str(root / (name + '.wav')), text=text)
    path = root / (name + '.json')
    path.write_text(json.dumps(value))
    path.chmod(0o600)
    return path, value

def launch(path):
    return subprocess.Popen([str(binary), '--request', str(path)], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)

def finish(process, timeout=180):
    try:
        return process.communicate(timeout=timeout)
    except BaseException:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise

cancel_request, cancel_value = request('cancelled')
process = launch(cancel_request)
lines = []
try:
    deadline = time.monotonic() + 180
    reached = False
    while time.monotonic() < deadline and process.poll() is None:
        ready, _, _ = select.select([process.stderr], [], [], 1)
        if not ready:
            continue
        line = process.stderr.readline()
        lines.append(line)
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get('event') == 'progress' and event.get('tokens') == 1:
            reached = True
            break
    assert reached, 'actual speech generation never reached token1'
    os.killpg(process.pid, signal.SIGTERM)
    stdout, tail = finish(process, 10)
finally:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
assert process.returncode == -signal.SIGTERM
assert not stdout.strip() and not Path(cancel_value['output']).exists()
assert not list(root.glob('.chatterbox-*.wav'))
(root / 'cancel.stderr.log').write_text(''.join(lines) + tail)
print('PASS real token1 reached, process terminated/reaped, no partial WAV', flush=True)

completed_request, completed_value = request('completed')
process = launch(completed_request)
stdout, stderr = finish(process)
(root / 'completed.stdout.json').write_text(stdout)
(root / 'completed.stderr.log').write_text(stderr)
assert process.returncode == 0, stderr
metadata = json.loads(stdout)
assert metadata['schemaVersion'] == 1 and metadata['sampleRate'] == 24000 and metadata['channels'] == 1
output = Path(completed_value['output'])
with wave.open(str(output), 'rb') as reader:
    assert (reader.getnchannels(), reader.getsampwidth(), reader.getframerate()) == (1, 2, 24000)
    assert reader.getnframes() == metadata['frames'] and reader.getnframes() >= 24000
assert not list(root.glob('.chatterbox-*.wav'))
assert output.stat().st_mode & 0o777 == 0o600
before = hashlib.sha256(output.read_bytes()).hexdigest()
process = launch(completed_request)
stdout, stderr = finish(process, 15)
assert process.returncode != 0 and not stdout.strip()
assert hashlib.sha256(output.read_bytes()).hexdigest() == before
invalid_request, invalid_value = request('unknown-field')
invalid_value['url'] = 'https://example.invalid/no-network'
invalid_request.write_text(json.dumps(invalid_value))
process = launch(invalid_request)
stdout, stderr = finish(process, 15)
assert process.returncode != 0 and not stdout.strip() and not Path(invalid_value['output']).exists()
report = dict(passed=True, output=str(output), sha256=before, frames=metadata['frames'], sampleRate=24000, cancellation='SIGTERM after actual token1', noReplace=True, unknownFieldsRefused=True)
(root / 'report.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report), flush=True)
