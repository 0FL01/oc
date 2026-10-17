import fs from 'node:fs';
import path from 'node:path';

// Official npm @opencode/cli-linux-x64@2.0.31, archive SRI independently verified.
// These four release sources are byte-identical to the explicitly supplied checkout.
export const mcpSidebarReference = {
  classification: 'SUPPLEMENTARY', version: '2.0.31',
  donor_commit: 'bc5ebf00a4c169ece10f276e6ce02eaf18c326e6',
  release_commit: '22728d67461260d56bb9796b9b8f36e1de47202e',
  binary_sha256: '2df936a20cf74af5706020342164628f9a31978c50c2b0ad306b42b614fe245d',
  source_hashes: {
    'packages/tui/src/feature-plugins/sidebar/mcp.tsx': '912929d010590a62d2a6a3354b15f64512bab53dc683a7d9dabe53e62b2282f4',
    'packages/tui/src/routes/session/sidebar.tsx': '5ec43052331af28a187826571e27864ccfe77f0c8956d58f3bd8cb10251d5450',
    'packages/tui/src/feature-plugins/sidebar/context.tsx': '3d654fbab1bd104a6cf4c4431b32e5367803613ccda7ced4c251adf9b39b37d0',
    'packages/tui/src/component/dialog-mcp.tsx': '6a05dfa26984be7f981abbddb1787a8f33c8c9d3cbbd8bc1d0c94b9ee7cac084',
  },
};

export async function probeMcpSidebar({origin, dir, send, waitFor, capture, visibleMatches, logs, control, prompt}) {
  const checks = {status: 'RUNNING', origin, classification: 'SUPPLEMENTARY', checks: []};
  const save = () => fs.writeFileSync(path.join(dir, 'mcp-sidebar-checks.json'), JSON.stringify(checks, null, 2) + '\n');
  const check = (name, passed, observations) => {
    checks.checks.push({name, passed, observations}); save();
    if (!passed) throw Error('MCP sidebar check failed: ' + name);
  };
  const sidebar = (f, needle) => visibleMatches(f, needle).filter(p => p.x >= f.columns - 42 && p.y > 0);
  const entries = [['chrome-devtools', 'Disabled'], ['codex_web', 'Connected'], ['crw', 'Disabled']];
  const rows = f => entries.map(([name, label]) => {
    const names = sidebar(f, name);
    if (names.length !== 1) return null;
    const at = names[0];
    const statuses = sidebar(f, label).filter(p => p.y === at.y);
    if (statuses.length !== 1) return null;
    return {name, label, at, status: statuses[0], dot: f.cells[at.y][at.x - 2],
      name_cell: f.cells[at.y][at.x], status_cell: f.cells[at.y][statuses[0].x]};
  });
  try {
    send(prompt, 'MCP_SIDEBAR_FIXTURE_PROMPT');
    send('\r', 'MCP_SIDEBAR_SUBMIT');
    const expanded = await waitFor(f => sidebar(f, 'MCP').length === 1 && rows(f).every(Boolean) &&
      f.text.includes('GEOMETRY-SHORT: one short answer.') &&
      logs.filter(e => e.kind === 'provider_completed').length >= 2, 'settled live MCP sidebar');
    const observed = rows(expanded);
    check('expanded-ordered-styled-rows', observed.every((row, index) =>
      row.dot.symbol === '•' && row.name_cell.modifiers.includes('bold') &&
      !row.status_cell.modifiers.includes('bold') && (!index || row.at.y === observed[index - 1].at.y + 1)), observed);
    check('expanded-full-frame', await capture('mcp-sidebar-expanded-160x48', expanded, 'CAPTURED_SUPPLEMENTARY') !== 'UNSTABLE_CAPTURE');
    const providersBefore = logs.filter(e => e.kind === 'provider').length;
    const header = sidebar(expanded, 'MCP')[0];
    send(`\x1b[<0;${header.x + 1};${header.y + 1}M\x1b[<0;${header.x + 1};${header.y + 1}m`, 'MCP_SIDEBAR_FOLD');
    const folded = await waitFor(f => sidebar(f, 'MCP (1 active)').length === 1 &&
      entries.every(([name]) => sidebar(f, name).length === 0) && sidebar(f, 'Context').length === 1,
      'acknowledged folded MCP section');
    check('folded-full-frame', await capture('mcp-sidebar-folded-160x48', folded, 'CAPTURED_SUPPLEMENTARY') !== 'UNSTABLE_CAPTURE');
    check('fold-does-not-generate', providersBefore === logs.filter(e => e.kind === 'provider').length &&
      logs.filter(e => e.kind === 'provider').every(e => e.valid), {providers: providersBefore});
    const request = `${origin}-mcp-sidebar-effects`;
    control({kind: 'mcp_sidebar_snapshot', request_id: request});
    await waitFor(() => logs.some(e => e.kind === 'mcp_sidebar_snapshot' && e.request_id === request), 'MCP sidebar effect receipt');
    const snapshot = logs.find(e => e.kind === 'mcp_sidebar_snapshot' && e.request_id === request);
    check('one-real-client-disabled-zero-spawn-zero-tools', snapshot.spawns.length === 1 &&
      snapshot.spawns[0].role === 'healthy-peer' && snapshot.initializations.length === 1 &&
      snapshot.initializations[0].pid === snapshot.spawns[0].pid &&
      snapshot.initializations[0].role === 'healthy-peer' && snapshot.calls.length === 0, snapshot);
    send('/quit', 'MCP_SIDEBAR_QUIT_DRAFT');
    await waitFor(f => f.text.includes('/quit'), 'painted quit command');
    send('\r', 'MCP_SIDEBAR_QUIT');
    const deadline = Date.now() + 12000;
    while (!logs.some(e => e.kind === 'exit') && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
    check('owned-natural-exit', logs.some(e => e.kind === 'exit' && e.code === 0 && e.termination === 'natural'));
    checks.status = 'PASS'; save();
    return checks;
  } catch (error) {
    checks.status = 'FAILED'; checks.error = error.message; save(); throw error;
  }
}
