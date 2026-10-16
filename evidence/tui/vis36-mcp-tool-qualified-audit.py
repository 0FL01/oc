"""Fresh literal Prompt005 specialization; immutable Prompt004 observer/seal."""
import ast
from pathlib import Path

SOURCE = Path('/home/opencode/ai/oc/evidence/tui/vis36-mcp-tool-audit.py')
tree = ast.parse(SOURCE.read_bytes(), filename=str(SOURCE))
replacements = {'vis36-prompt-current-004/prompt-wide-r4': 'vis36-prompt-current-005/prompt-wide-r5',
                'vis36-prompt-current-004/prompt-small-r4': 'vis36-prompt-current-005/prompt-small-r5',
                'vis36-prompt-current-004': 'vis36-prompt-current-005',
                'vis36-prompt-capture-r4.py': 'vis36-prompt-capture-r5.py',
                'vis36-mcp-tool-before.json': 'vis36-mcp-tool-qualified-before.json',
                'vis36-mcp-tool-after.json': 'vis36-mcp-tool-qualified-after.json'}
hits = {key: 0 for key in replacements}
for node in ast.walk(tree):
    if isinstance(node, ast.Constant) and isinstance(node.value, str) and node.value in replacements:
        hits[node.value] += 1
        node.value = replacements[node.value]
assert set(hits.values()) == {1}
# Include the now-historical complete Prompt004 bytes and directory metadata.
legacy = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'legacy_seals')
attempts = [n for n in ast.walk(legacy) if isinstance(n, ast.Tuple) and
            [e.value for e in n.elts if isinstance(e, ast.Constant)] == ['001', '002', '003']]
assert len(attempts) == 1
attempts[0].elts.append(ast.Constant('004'))
exec(compile(ast.fix_missing_locations(tree), str(SOURCE), 'exec'))
