"""Supplementary 2.0.31 sidebar inventory; never the real codex_web service."""
import json
from pathlib import Path


def configure(spec, home, project, config, cli_config):
    (home / 'vis16-peer-phase.json').write_text(json.dumps({'phase': 'healthy'}))
    peer = str(Path(__file__).with_name('tool_preview_mcp.py'))
    config['mcp'] = {'servers': {
        name: {'type': 'local', 'disabled': name != 'codex_web',
               'command': ['/usr/bin/python3', str(Path(__file__)), peer, '--probe-role', role]}
        # Command arguments are protected credential-domain inputs in native;
        # never use the display label itself as a private probe argument.
        for name, role in (('chrome-devtools', 'disabled-one'),
                           ('codex_web', 'healthy-peer'), ('crw', 'disabled-two'))
    }}
    config['snapshots'] = False
    cli_config['animations'] = False
    cli_config['session'].update(sidebar='auto', tps=False)


def snapshot(home):
    def records(name):
        file = home / name
        return [json.loads(line) for line in file.read_text().splitlines()] if file.exists() else []
    return {'spawns': records('mcp-sidebar-spawns.jsonl'),
            'initializations': records('vis16-mcp-lifecycle.jsonl'),
            'calls': records('vis16-mcp.jsonl')}


if __name__ == '__main__':
    import os
    import runpy
    import sys

    # Record process entry before the peer reads any initialize request.
    with (Path(os.environ['HOME']) / 'mcp-sidebar-spawns.jsonl').open('a') as log:
        log.write(json.dumps({'role': sys.argv[-1], 'pid': os.getpid()}) + '\n')
    runpy.run_path(sys.argv.pop(1), run_name='__main__')
