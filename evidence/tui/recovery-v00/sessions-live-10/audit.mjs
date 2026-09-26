// Read-only evidence audit. All equality outcomes remain whole-frame comparisons.
import fs from 'node:fs';
import path from 'node:path';
const root = path.dirname(new URL(import.meta.url).pathname);
const read = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
const lock = read('capture.lock.json');
const counts = Object.fromEntries(['EQUAL', 'DIFFERENT', 'BLOCKED', 'INVALID'].map((s) => [s, lock.attempts.filter((a) => a.status === s).length]));
console.log(JSON.stringify({ comparisons: counts, modes: Object.fromEntries(['grid', 'png'].map((m) => [m, lock.attempts.filter((a) => a.mode === m).length])) }, null, 2));
for (const origin of ['upstream', 'oc']) {
  const c = read(`${origin}/sessions-checks.json`);
  const rows = (stage) => c.checks.find((a) => a.stage === stage).observation.observations.flatMap((o) => o.rows);
  const id = c.selected_fixture.id;
  const stages = ['all-before-actions', 'renamed-db', 'armed-db', 'deleted-db'];
  const effects = Object.fromEntries(stages.map((s) => [s, rows(s)]));
  const unchanged = c.checks.filter((s) => s.provider_unchanged !== undefined).every((s) => s.provider_unchanged && JSON.stringify(s.provider_counts) === JSON.stringify(c.baseline));
  const deletedOnlySelected = JSON.stringify(rows('armed-db').filter((r) => r.id !== id)) === JSON.stringify(rows('deleted-db'));
  console.log(JSON.stringify({ origin, status: c.status, baseline: c.baseline, provider_unchanged: unchanged, deleted_only_selected: deletedOnlySelected, captured: c.checks.filter((s) => s.capture_status === 'CAPTURED').length, effects }, null, 2));
  if (c.status !== 'PASS' || !unchanged || !deletedOnlySelected) process.exitCode = 1;
}
for (const stage of ['delete-confirm', 'selected-root-cwd', 'rename-input', 'cwd-scope']) {
  const a = read(`upstream/sessions-${stage}.cells.json`);
  const b = read(`oc/sessions-${stage}.cells.json`);
  const rows = a.cells.map((row, y) => ({ y, different_cells: row.filter((cell, x) => JSON.stringify(cell) !== JSON.stringify(b.cells[y][x])).length })).filter((r) => r.different_cells);
  console.log(JSON.stringify({ stage, whole_frame_differing_rows: rows, grid: read(`sessions-${stage}.grid-diff.json`).different_cells, png: read(`sessions-${stage}.png-diff.json`).different_pixels }, null, 2));
  if (stage === 'delete-confirm' || stage === 'selected-root-cwd') {
    const fields = new Map();
    a.cells.forEach((row, y) => row.forEach((cell, x) => {
      for (const field of ['fg', 'bg', 'modifiers']) {
        if (JSON.stringify(cell[field]) === JSON.stringify(b.cells[y][x][field])) continue;
        const key = JSON.stringify({ y, field, reference: cell[field], actual: b.cells[y][x][field] });
        const group = fields.get(key) ?? [];
        group.push(x);
        fields.set(key, group);
      }
    }));
    console.log(JSON.stringify({ stage, whole_frame_style_groups: [...fields].map(([key, x]) => ({ ...JSON.parse(key), x })) }, null, 2));
  }
}
