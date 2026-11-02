#!/usr/bin/env python3
"""Private REA stdio client with explicit long deadlines, no Codex config edits.

Runs a reduced auto-analysis profile, retains actual tools/list schemas and
per-operation Evidence, and closes its owned server/session. Target paths are
explicit and shipped target content is never executed.
"""
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
import tempfile

ROOT = Path(__file__).resolve().parents[1]
QUAL = ROOT / 'docs/qualification/insta360-2026-10-31'
BASE = QUAL / 'focused-rea'
BASE.mkdir(parents=True, exist_ok=True)
OUT = Path(tempfile.mkdtemp(prefix='run-', dir=BASE))
env = os.environ.copy()
env.update(GHIDRA_INSTALL_DIR='/Users/themoretheless/Applications/ghidra_12.1.4_PUBLIC',
           JAVA_HOME='/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home',
           REA_GHIDRA_STARTUP_TIMEOUT_MS='1800000',
           REA_GHIDRA_DISABLED_ANALYZERS='Decompiler Switch Analysis,Basic Constant Reference Analyzer,Objective-C Message Analyzer')
stderr = (OUT / 'server.stderr.log').open('w')
server = subprocess.Popen(['/opt/homebrew/bin/rea', 'mcp'], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=stderr, text=True,
                          env=env, bufsize=1)
responses = queue.Queue()


def read_responses():
    try:
        for line in server.stdout:
            try:
                responses.put(json.loads(line))
            except json.JSONDecodeError:
                responses.put({'transport_error': line[:2000]})
    finally:
        responses.put({'transport_closed': True})


threading.Thread(target=read_responses, daemon=True).start()
request_id = 0


def record(stage, **details):
    row = {'time': time.time(), 'stage': stage, **details}
    with (OUT / 'progress.jsonl').open('a') as log:
        log.write(json.dumps(row) + '\n')
    print(json.dumps(row), flush=True)


def send(value):
    server.stdin.write(json.dumps(value) + '\n')
    server.stdin.flush()


def rpc(method, params, timeout=1950):
    global request_id
    request_id += 1
    selected = request_id
    send({'jsonrpc': '2.0', 'id': selected, 'method': method, 'params': params})
    end = time.monotonic() + timeout
    while True:
        message = responses.get(timeout=max(.01, end - time.monotonic()))
        if 'transport_error' in message or 'transport_closed' in message:
            raise RuntimeError(message)
        if message.get('id') == selected:
            if 'error' in message:
                raise RuntimeError(message['error'])
            return message['result']


def tool(name, arguments, filename):
    record('start', tool=name, output=filename)
    result = rpc('tools/call', {'name': name, 'arguments': arguments})
    (OUT / filename).write_text(json.dumps(result, indent=2))
    record('finish', tool=name, is_error=bool(result.get('isError')))
    return result


opened = False
try:
    init = rpc('initialize', {'protocolVersion': '2024-11-05',
                              'capabilities': {},
                              'clientInfo': {'name': 'video-kadr-qualification', 'version': '1'}})
    (OUT / 'initialize.json').write_text(json.dumps(init, indent=2))
    send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
    catalog = rpc('tools/list', {})
    (OUT / 'tools.json').write_text(json.dumps(catalog, indent=2))
    available = {t['name'] for t in catalog['tools']}
    assert {'open_binary', 'binary_overview', 'procedure_pseudo_code',
            'export_evidence_bundle', 'close_binary'} <= available
    result = tool('open_binary', {'path': '/Applications/Insta360 Studio.app',
                                 'provider_id': 'ghidra'}, 'open.json')
    if result.get('isError'):
        raise RuntimeError('REA could not open target; see retained response')
    opened = True
    result = tool('binary_overview', {}, 'overview.json')
    if result.get('isError'):
        raise RuntimeError('Reduced-profile overview failed; see retained response')
    targets = json.loads((QUAL / 'deep-native-targets.json').read_text())['targets']
    for i, target in enumerate(targets):
        tool('procedure_pseudo_code', {'procedure': target['address']}, f'procedure-{i}.json')
    for pattern in ['GyroDataParser', 'StitchingOptimizationViewModel', 'DeepTrackConfiguration']:
        tool('search_procedures', {'pattern': pattern, 'detail': 'summary'},
             f'search-{pattern}.json')
    tool('export_evidence_bundle', {'path': str(OUT / 'evidence.json')}, 'export.json')
    tool('close_binary', {'snapshot_path': str(OUT / 'analysis.snapshot.json')}, 'close.json')
    opened = False
    record('complete')
finally:
    if opened:
        try:
            tool('export_evidence_bundle', {'path': str(OUT / 'partial-evidence.json')}, 'partial-export.json')
            tool('close_binary', {}, 'cleanup.json')
        except Exception as error:
            record('cleanup-error', message=str(error))
    server.stdin.close()
    try:
        server.wait(timeout=30)
    except subprocess.TimeoutExpired:
        server.terminate()
        server.wait(timeout=30)
    stderr.close()
