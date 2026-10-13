"""Verify the already-durable BEFORE after a telemetry-only missing status key; no replay."""
import importlib.util
import json
from pathlib import Path

path = Path('/home/opencode/ai/oc/evidence/tui/vis36-prompt-audit.py')
spec = importlib.util.spec_from_file_location('sealed_prompt_observer', path)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
before = audit.check.load(audit.BEFORE)
assert 'status' not in before
assert audit.original_inventory() == before['original_inventory']
assert audit.source_and_elf() == before['source_and_elf']
assert all(dict(sha256=audit.check.digest(p), metadata=audit.check.metadata(p)) == record
           for group in ('helpers', 'prior_proofs') for p, record in before[group].items())
assert {role: {side: audit.effects(audit.ROOTS[role], side) for side in ('oc', 'upstream')}
        for role in ('current-wide', 'current-small')} == before['effects']
audit.save(audit.OUT / 'vis36-prompt-before-verification.json', {
    'status': 'INDEPENDENT_PROMPT_SEALED_BEFORE_VERIFIED_NO_REPLAY',
    'original_before_sha256': audit.check.digest(audit.BEFORE),
    'sealed_program_sha256': audit.check.digest(path),
    'verifier_sha256': audit.check.digest(Path(__file__)),
    'cause': 'BEFORE was exclusively written and fsynced, then telemetry print accessed absent status. All sealed facts independently revalidated; neither program nor BEFORE rewritten.',
    'per_archive_original_files': {p: len(i['files']) for p, i in before['original_inventory'].items()},
    'preserved_prior_proofs': len(before['prior_proofs']),
})
