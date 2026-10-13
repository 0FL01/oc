"""Independent current003 inverse; retain sealed002 without entrypoint replay."""
import ast
import collections
from pathlib import Path

SOURCE = Path('/home/opencode/ai/oc/evidence/tui/vis41-busy-hover-audit.py')
tree = ast.parse(SOURCE.read_text())
replacements = {
    'vis36-prompt-current-002/prompt-wide-r2': 'vis36-prompt-current-003/prompt-wide-r3',
    'vis36-prompt-current-002/prompt-small-r2': 'vis36-prompt-current-003/prompt-small-r3',
    'vis36-prompt-current-002': 'vis36-prompt-current-003',
    'vis36-prompt-capture-r2.py': 'vis36-prompt-capture-r3.py',
    'vis41-busy-hover-before.json': 'vis41-busy-hover-final-before.json',
    'vis41-busy-hover-after.json': 'vis41-busy-hover-final-after.json',
    'vis36-busy-hover-': 'vis36-busy-hover-r3-',
    'INDEPENDENT_BUSY_HOVER_BEFORE_SEALED': 'INDEPENDENT_CURRENT003_BUSY_HOVER_BEFORE_SEALED',
    'INDEPENDENT_BUSY_HOVER_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS':
        'INDEPENDENT_CURRENT003_BUSY_HOVER_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS',
}
seen = collections.Counter()
for node in ast.walk(tree):
    if isinstance(node, ast.Constant) and isinstance(node.value, str) and node.value in replacements:
        seen[node.value] += 1
        node.value = replacements[node.value]
assert dict(seen) == {value: 1 for value in replacements}

original_changed = {
    'crates/oc-tui/src/app/tabs.rs', 'crates/oc-tui/src/app/tests/tabs.rs',
    'crates/oc-tui/src/shell.rs', 'crates/oc-tui/src/shell/tests.rs',
}
changed = [node for node in ast.walk(tree) if isinstance(node, ast.Set)
           and all(isinstance(item, ast.Constant) and isinstance(item.value, str) for item in node.elts)
           and {item.value for item in node.elts} == original_changed]
assert len(changed) == 1
changed[0].elts.append(ast.Constant(value='crates/oc/tests/pty_t39/lifecycle.rs'))

before = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'before']
assert len(before) == 1
helpers = [node for node in before[0].body if isinstance(node, ast.Assign)
           and any(isinstance(target, ast.Name) and target.id == 'helpers' for target in node.targets)]
assert len(helpers) == 1 and isinstance(helpers[0].value, ast.List)
helpers[0].value.elts.append(ast.Call(func=ast.Name(id='Path', ctx=ast.Load()),
                                    args=[ast.Constant(value=str(SOURCE))], keywords=[]))

# Execute only the new source003 specialization: new exclusive report names,
# original manual reader and all byte, field, effect, source and quota guards.
# SOURCE itself and every sealed002 report/capture remain immutable.
exec(compile(ast.fix_missing_locations(tree), str(SOURCE), 'exec'), globals())
