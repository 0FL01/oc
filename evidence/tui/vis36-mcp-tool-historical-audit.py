"""Read-only corrected observer for sealed Prompt004 after the test-only lint fix."""
import ast
from pathlib import Path

SOURCE = Path('/home/opencode/ai/oc/evidence/tui/vis36-mcp-tool-audit.py')
tree = ast.parse(SOURCE.read_bytes(), filename=str(SOURCE))
entrypoints = [n for n in tree.body if isinstance(n, ast.If) and isinstance(n.test, ast.Compare) and
               isinstance(n.test.left, ast.Name) and n.test.left.id == '__name__']
assert len(entrypoints) == 1
tree.body.remove(entrypoints[0])
for node in ast.walk(tree):
    if isinstance(node, ast.Constant) and node.value == 'vis36-mcp-tool-after.json':
        node.value = 'vis36-mcp-tool-historical-after.json'
    elif isinstance(node, ast.Constant) and node.value == 'INDEPENDENT_MCP_TOOL_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS':
        node.value = 'HISTORICAL_PROMPT004_TEST_ONLY_CLIPPY_CORRECTION_FULL_BYTE_EXACT_T44_ACTIVE_NOT_PASS'
    elif isinstance(node, ast.Constant) and node.value == 'vis36-mcp-tool-':
        node.value = 'vis36-mcp-tool-r4-'
exec(compile(tree, str(SOURCE), 'exec'))

def source_and_elf():
    captured = check.load(BEFORE)['source_and_elf']
    qualified = check.load(OUT / 'vis36-mcp-tool-qualified-before.json')['source_and_elf']
    assert v.source() == qualified['source'] and set(captured['source']) == set(qualified['source'])
    assert [p for p in captured['source'] if captured['source'][p] != qualified['source'][p]] == [
        'crates/oc-core/src/approval_tool_tests.rs']
    assert captured['elf'] == qualified['elf']
    assert {side: v.frames.elf(Path(row['path'])) for side, row in captured['elf'].items()} == captured['elf']
    # This is explicitly a historical seal, not a current-source qualification.
    # Its original captured flags/manifest, clocks, bytes and metadata stay raw.
    return captured

if __name__ == '__main__':
    assert sys.argv[1:] == ['after']
    after()
