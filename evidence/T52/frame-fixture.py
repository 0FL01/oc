"""T52 deterministic render fixture, no new Cargo target or source API."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
phase = sys.argv[1]
assert phase in ('before', 'after')
out = ROOT / 'evidence/T52' / f'frames-{phase}.json'
assert not out.exists(), out
completed = subprocess.run(['cargo', 'build', '-p', 'oc-tui', '--locked', '--message-format=json'],
    cwd=ROOT, text=True, capture_output=True)
assert completed.returncode == 0, completed.stderr
libraries = {}
for line in completed.stdout.splitlines():
    message = json.loads(line)
    if message.get('reason') == 'compiler-artifact' and 'lib' in message['target']['kind']:
        artifacts = [p for p in message['filenames'] if p.endswith('.rlib')]
        if artifacts:
            libraries[message['target']['name']] = artifacts[0]
binary = TMP / f't52-frames-{phase}'
command = ['rustc', '--edition=2024', str(ROOT / 'evidence/T52/frame-fixture.rs'), '-o', str(binary),
    '-L', f'dependency={ROOT / "target/debug/deps"}']
for name in ('oc_core', 'oc_tui', 'ratatui', 'tokio', 'serde_json', 'unicode_width'):
    command.extend(['--extern', name + '=' + libraries[name]])
subprocess.run(command, check=True, cwd=ROOT)
raw = subprocess.check_output([str(binary)], cwd=ROOT, text=True)
frames = json.loads(raw)
out.write_text(json.dumps({'kind':'T52 in-memory existing render-alloc fixture; NOT PTY or T44 parity',
    'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
    'commands':['cargo build -p oc-tui --locked --message-format=json',command,str(binary)],
    'frames':frames}, ensure_ascii=False) + '\n')
print(f'{phase}: {len(frames)} full frames, {sum(f["columns"] * f["rows"] for f in frames)} cells; {out}')
