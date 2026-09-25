#!/usr/bin/env node
// Reference-only runner: dependencies live outside the production workspace.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawn, spawnSync} from 'node:child_process';
import readline from 'node:readline';
import {fileURLToPath} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const args = Object.fromEntries(process.argv.slice(2).map((v, i, a) => v.startsWith('--') ? [v.slice(2), a[i+1]] : null).filter(Boolean));
const tools = path.resolve(args.tools || '/home/opencode/.cache/opencode-tmp/opencode/t44-reference');
process.env.PLAYWRIGHT_BROWSERS_PATH = path.join(tools, 'browsers');
const require = createRequire(path.join(tools, 'package.json'));
const {chromium} = require('playwright');
const explorationClick = args['exploration-click'] === 'true';
if (args['exploration-click'] !== undefined && !['true','false'].includes(args['exploration-click']))
  throw Error('--exploration-click must be true or false');
const tabClick = args['tab-click'] === 'true';
if (args['tab-click'] !== undefined && !['true','false'].includes(args['tab-click']))
  throw Error('--tab-click must be true or false');
const tabClose = args['tab-close'] === 'true';
if (args['tab-close'] !== undefined && !['true','false'].includes(args['tab-close']))
  throw Error('--tab-close must be true or false');
if (tabClose && !tabClick) throw Error('--tab-close true requires --tab-click true (paired Reader tools 120x40 profile)');
const tabCloseKey = args['tab-close-key'] === 'true';
if (args['tab-close-key'] !== undefined && !['true','false'].includes(args['tab-close-key']))
  throw Error('--tab-close-key must be true or false');
if (tabCloseKey && (!tabClick || tabClose))
  throw Error('--tab-close-key true requires --tab-click true without --tab-close true (paired Reader tools 120x40 profile)');
const tabRestart = args['tab-restart'] === 'true';
if (args['tab-restart'] !== undefined && !['true','false'].includes(args['tab-restart']))
  throw Error('--tab-restart must be true or false');
if (tabRestart && (!tabClick || tabClose || tabCloseKey || explorationClick))
  throw Error('--tab-restart true requires --tab-click true without tab-close/tab-close-key/exploration-click');
const renameSession = args['rename-session'] === 'true';
if (args['rename-session'] !== undefined && !['true','false'].includes(args['rename-session']))
  throw Error('--rename-session must be true or false');
const regenerateTitle = args['regenerate-title'] === 'true';
if (args['regenerate-title'] !== undefined && !['true','false'].includes(args['regenerate-title']))
  throw Error('--regenerate-title must be true or false');
const sidebarPalette = args['sidebar-palette'] === 'true';
if (args['sidebar-palette'] !== undefined && !['true','false'].includes(args['sidebar-palette']))
  throw Error('--sidebar-palette must be true or false');
const autocomplete = args.autocomplete === 'true';
if (args.autocomplete !== undefined && !['true','false'].includes(args.autocomplete))
  throw Error('--autocomplete must be true or false');
const autocompleteKeys = args['autocomplete-keys'] === 'true';
if (args['autocomplete-keys'] !== undefined && !['true','false'].includes(args['autocomplete-keys']))
  throw Error('--autocomplete-keys must be true or false');
const ctrlC = args['ctrl-c'] === 'true';
if (args['ctrl-c'] !== undefined && !['true','false'].includes(args['ctrl-c']))
  throw Error('--ctrl-c must be true or false');
const twoTurn = args['two-turn'] === 'true';
if (args['two-turn'] !== undefined && !['true','false'].includes(args['two-turn']))
  throw Error('--two-turn must be true or false');
const modelsInteraction = args['models-interaction'] === 'true';
if (args['models-interaction'] !== undefined && !['true','false'].includes(args['models-interaction']))
  throw Error('--models-interaction must be true or false');
const autocompleteKeysRename = args['autocomplete-keys-rename'] === 'true';
if (args['autocomplete-keys-rename'] !== undefined && !['true','false'].includes(args['autocomplete-keys-rename']))
  throw Error('--autocomplete-keys-rename must be true or false');
if (autocompleteKeysRename && !autocompleteKeys)
  throw Error('--autocomplete-keys-rename true requires --autocomplete-keys true');
const autocompleteKeysMove = args['autocomplete-keys-move'] === 'true';
if (args['autocomplete-keys-move'] !== undefined && !['true','false'].includes(args['autocomplete-keys-move']))
  throw Error('--autocomplete-keys-move must be true or false');
if (autocompleteKeysMove && !autocompleteKeys)
  throw Error('--autocomplete-keys-move true requires --autocomplete-keys true');
const mention = args.mention === 'true';
if (args.mention !== undefined && !['true','false'].includes(args.mention))
  throw Error('--mention must be true or false');
const reasoningClick = args['reasoning-click'] === 'true';
if (args['reasoning-click'] !== undefined && !['true','false'].includes(args['reasoning-click']))
  throw Error('--reasoning-click must be true or false');
const reasoningSteps = args['reasoning-steps'] === 'true';
if (args['reasoning-steps'] !== undefined && !['true','false'].includes(args['reasoning-steps']))
  throw Error('--reasoning-steps must be true or false');
const reasoningReleaseOnly = args['reasoning-release-only'] === 'true';
if (args['reasoning-release-only'] !== undefined && !['true','false'].includes(args['reasoning-release-only']))
  throw Error('--reasoning-release-only must be true or false');
if (reasoningReleaseOnly && !reasoningClick)
  throw Error('--reasoning-release-only true requires --reasoning-click true (paired Reader reasoning 120x40 profile)');
const selectionCopy = args['selection-copy'] === 'true';
if (args['selection-copy'] !== undefined && !['true','false'].includes(args['selection-copy']))
  throw Error('--selection-copy must be true or false');
const toastOverlap = args['toast-overlap'] === 'true';
if (args['toast-overlap'] !== undefined && !['true','false'].includes(args['toast-overlap']))
  throw Error('--toast-overlap must be true or false');
const scanner = args.scanner === 'true';
if (args.scanner !== undefined && !['true','false'].includes(args.scanner))
  throw Error('--scanner must be true or false');
if (scanner && !['true','false'].includes(args['scanner-animation']))
  throw Error('--scanner true requires --scanner-animation true or false');
if (!scanner && args['scanner-animation'] !== undefined)
  throw Error('--scanner-animation requires --scanner true');
const scannerAnimation = args['scanner-animation'] === 'true';
if (scanner && !['true','false'].includes(args['scanner-cancel']))
  throw Error('--scanner true requires --scanner-cancel true or false');
if (!scanner && args['scanner-cancel'] !== undefined)
  throw Error('--scanner-cancel requires --scanner true');
const scannerCancel = args['scanner-cancel'] === 'true';
if (modelsInteraction && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || ![120,160].includes(Number(args.columns)) ||
    Number(args.rows)!== (Number(args.columns)===120 ? 40 : 48) || !args.reference || !args.oc ||
    args.matrix === 'true' || args.variants === 'true' || args['scroll-resize'] === 'true' ||
    args['startup-error'] === 'true' || args['seed-root'] || args.session || args.tabs === 'vertical' ||
    [explorationClick,tabClick,tabClose,tabCloseKey,tabRestart,renameSession,regenerateTitle,
      sidebarPalette,autocomplete,autocompleteKeys,ctrlC,twoTurn,mention,reasoningClick,
      reasoningSteps,reasoningReleaseOnly,selectionCopy,toastOverlap,scanner].some(Boolean)))
  throw Error('--models-interaction true requires only paired Reader/tools 120x40 or 160x48 --geometry true --sidebar hide --agent-profile true');
if (reasoningSteps && (args.geometry !== 'true' || args.sample !== 'reasoning-steps' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] || args.session ||
    args.tabs === 'vertical' || explorationClick || tabClick || tabClose || tabCloseKey || tabRestart ||
    renameSession || regenerateTitle || sidebarPalette || autocomplete || autocompleteKeys ||
    autocompleteKeysRename || autocompleteKeysMove || mention || reasoningClick || reasoningReleaseOnly ||
    selectionCopy || toastOverlap || scanner || ctrlC || twoTurn))
  throw Error('--reasoning-steps true requires only paired Reader/reasoning-steps 120x40 --geometry true --sidebar hide --agent-profile true');
if (args.sample === 'reasoning-steps' && !reasoningSteps)
  throw Error('--sample reasoning-steps requires --reasoning-steps true');
if (reasoningReleaseOnly && (selectionCopy || toastOverlap || scanner || ctrlC || twoTurn))
  throw Error('--reasoning-release-only true cannot be combined with other interaction modes');
if (toastOverlap && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'auto' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 121 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args['build-oc'] === 'true' || args['refresh-before-capture'] === 'true' ||
    args.devtools === 'true' || args.session || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || renameSession || sidebarPalette ||
    regenerateTitle || autocomplete || autocompleteKeys || mention || reasoningClick || selectionCopy ||
    scanner || ctrlC || twoTurn))
  throw Error('--toast-overlap true requires only paired Reader/tools 121x40 --geometry true --sidebar auto --agent-profile true');
if (twoTurn && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || renameSession || sidebarPalette ||
    regenerateTitle || autocomplete || autocompleteKeys || mention || reasoningClick || selectionCopy || toastOverlap || scanner || ctrlC))
  throw Error('--two-turn true requires only paired Reader/tools 120x40 --geometry true --sidebar hide --agent-profile true');
if (ctrlC && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || renameSession || sidebarPalette ||
    regenerateTitle || autocomplete || autocompleteKeys || mention || reasoningClick || selectionCopy || toastOverlap || scanner))
  throw Error('--ctrl-c true requires paired Reader/tools 120x40, --geometry true --sidebar hide --agent-profile true and no other interaction/resize modes');
if (scanner && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || renameSession || sidebarPalette ||
    regenerateTitle || autocomplete || autocompleteKeys || mention || reasoningClick || selectionCopy || toastOverlap))
  throw Error('--scanner true requires paired Reader/tools 120x40, --geometry true --sidebar hide --agent-profile true and no other interaction/resize modes');
if (selectionCopy && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || renameSession || sidebarPalette ||
    regenerateTitle || autocomplete || autocompleteKeys || mention || reasoningClick || toastOverlap))
  throw Error('--selection-copy true requires paired binaries, --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40, horizontal tabs and no other interaction/resize modes');
if (reasoningClick && (args.geometry !== 'true' || args.sample !== 'reasoning' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || explorationClick || tabClick || tabClose || tabCloseKey || tabRestart ||
    renameSession || regenerateTitle || sidebarPalette || autocomplete || autocompleteKeys || mention || toastOverlap))
  throw Error('--reasoning-click true requires paired binaries, --geometry true --sample reasoning --sidebar hide --agent-profile true --columns 120 --rows 40, horizontal tabs and no other interaction/resize modes');
if (mention && autocomplete) throw Error('--mention true and --autocomplete true are mutually exclusive');
if (autocompleteKeys && (autocomplete || mention))
  throw Error('--autocomplete-keys true is mutually exclusive with --autocomplete true and --mention true');
if ((autocomplete || mention || autocompleteKeys) && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || tabClick || explorationClick || renameSession || sidebarPalette || toastOverlap ||
    regenerateTitle || tabClose || tabCloseKey || tabRestart))
  throw Error('--autocomplete/--mention/--autocomplete-keys true requires paired binaries, --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 and no other interaction/resize modes');
if (sidebarPalette && (args.geometry !== 'true' || args.sample !== 'tools' ||
    !['hide','auto'].includes(args.sidebar) || Number(args.columns) !== 160 || Number(args.rows) !== 48 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || args['tab-click'] === 'true' || args['exploration-click'] === 'true' ||
    args['rename-session'] === 'true' || toastOverlap))
  throw Error('--sidebar-palette true requires paired binaries, --geometry true --sample tools --sidebar hide|auto --columns 160 --rows 48, horizontal tabs and no other interaction/resize modes');
if (regenerateTitle && !renameSession)
  throw Error('--regenerate-title true requires --rename-session true (paired Reader tools 120x40 profile)');
if (renameSession && (tabClick || tabClose || tabCloseKey || tabRestart || explorationClick ||
    args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    args.matrix === 'true' || args.variants === 'true' || args['scroll-resize'] === 'true' || args['startup-error'] === 'true' ||
    args['seed-root'] || args.tabs === 'vertical' || toastOverlap || !args.reference || !args.oc))
  throw Error('--rename-session true requires both binaries, --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40, horizontal tabs and no tab-click/tab-close/tab-close-key/tab-restart/exploration-click/matrix/variants/scroll-resize/startup-error/seed-root');
if (explorationClick && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    Number(args.columns) !== 120 || Number(args.rows) !== 40 || args.matrix === 'true' || args['scroll-resize'] === 'true'))
  throw Error('--exploration-click true requires --geometry true --sample tools --sidebar hide --columns 120 --rows 40 without matrix/scroll-resize');
if (tabClick && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    args.matrix === 'true' || args['scroll-resize'] === 'true' || args['startup-error'] === 'true' ||
    args['seed-root'] || args.tabs === 'vertical' || !args.reference || !args.oc))
  throw Error('--tab-click true requires both binaries, --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40, horizontal tabs and no matrix/scroll-resize/startup-error/seed-root');
if (args.sample === 'rows-reflow' && args.matrix === 'true')
  throw Error('--sample rows-reflow does not support --matrix true; use --sample rows for the matrix/clamp run');
const output = path.resolve(args.output || path.join(repo, 'evidence/tui/recovery-v00', new Date().toISOString().replaceAll(':', '-')));
fs.mkdirSync(path.dirname(output), {recursive: true});
try { fs.mkdirSync(output); }
catch (e) { if (e.code === 'EEXIST') throw Error('Refusing to overwrite attempt: ' + output); throw e; }
const sha = data => createHash('sha256').update(data).digest('hex');
const canonical = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a],[b]) => a.localeCompare(b))) : v);
const json = (name, value) => fs.writeFileSync(path.join(output, name), JSON.stringify(value, null, 2) + '\n');
const fixture = path.join(repo, 'tui-recovery/fixtures');
const fixtureFiles = Object.fromEntries(fs.readdirSync(fixture).sort().map(n => [n, sha(fs.readFileSync(path.join(fixture,n)))]));
const fixtureSha = sha(canonical({files: fixtureFiles, sample: args.sample || 'table', variants: args.variants === 'true',
  models_interaction:modelsInteraction, protocol: sha(fs.readFileSync(path.join(here,'bridge.py')))}));
const commands = [];
commands.push({argv:[process.execPath,...process.argv.slice(1)],role:'capture runner invocation',exit_code:null});
const execute = (argv, options={}) => {
  const r = spawnSync(argv[0], argv.slice(1), {cwd: repo, encoding: 'utf8', ...options});
  commands.push({argv, exit_code: r.status, stdout: r.stdout, stderr: r.stderr});
  json('commands.json', commands);
  return r;
};
const commit = execute(['git', 'rev-parse', 'HEAD']).stdout.trim();
const tree = execute(['git', 'rev-parse', 'HEAD^{tree}']).stdout.trim();
const diff = execute(['git', 'diff', '--binary']).stdout;
// Seal actual inputs, including new Rust modules not yet staged. Deliberately
// restrict discovery to source/tool paths: never inspect authoring .opencode or
// product credentials. HEAD + tracked diff alone does not attest untracked code.
const sourcePaths = execute(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--',
  'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'scripts/tui_capture']);
if (sourcePaths.status !== 0) throw Error('Source input enumeration failed');
const sourceManifest = Object.fromEntries([...new Set(sourcePaths.stdout.split('\0').filter(Boolean))].sort()
  .filter(name => fs.existsSync(path.join(repo,name)))
  .map(name => [name,sha(fs.readFileSync(path.join(repo,name)))]));
json('source-manifest.json', sourceManifest);
const lock = {schema_version: 1, started: new Date().toISOString(), runner_version: 1,
  runner_hashes: Object.fromEntries(['capture.mjs','frontend.js','bridge.py'].map(n => [n, sha(fs.readFileSync(path.join(here,n)))])),
  fixture_sha256: fixtureSha, fixture_files: fixtureFiles, oc: {commit, tree, dirty_diff_sha256: sha(diff),
    source_manifest_sha256: sha(canonical(sourceManifest))},
  sources: {upstream_commit: '2670273ff17da96f85c5826ced57aa1b368754fa'}, attempts: [], captures: []};
if(args['build-oc'] === 'true') {
  const build = execute(['cargo', 'build', '--locked']);
  if(build.status !== 0) throw Error('Rust build failed (see commands.json)');
  lock.oc.build_command = ['cargo','build','--locked'];
} else lock.oc.build_provenance = 'Existing binary; build/source association not attested by this run';
fs.copyFileSync(path.join(tools, 'package-lock.json'), path.join(output, 'tooling.package-lock.json'));
const isolated = path.join(tools, 'runs', path.basename(output));
const cleanEnv = {PATH: '/usr/bin:/bin', HOME: path.join(isolated, 'browser-home'),
  LANG: 'C.UTF-8', LC_ALL: 'C.UTF-8', TZ: 'UTC', PLAYWRIGHT_BROWSERS_PATH: path.join(tools, 'browsers')};
fs.mkdirSync(cleanEnv.HOME, {recursive: true});
process.env.PLAYWRIGHT_BROWSERS_PATH = cleanEnv.PLAYWRIGHT_BROWSERS_PATH;
let browser;
let result = 0;
const sleep = ms => new Promise(r => setTimeout(r, ms));
// Match cell symbols, not JS string offsets (which differ from terminal columns
// for wide glyphs). A unique visible match is required before sending a click.
const visibleMatches = (f, needle) => {
  const symbols = [...needle];
  const matches = [];
  for (const [y, row] of f.cells.entries()) {
    for (let x=0; x<=row.length-symbols.length; x++) {
      if (symbols.every((symbol, i) => row[x+i].symbol===symbol && row[x+i].width===1))
        matches.push({x,y});
    }
  }
  return matches;
};
const tabTitle = JSON.parse(fs.readFileSync(path.join(fixture,'scenarios.json'),'utf8')).base.title;
const oldTitlePrefix = [...tabTitle].slice(0, 6).join('');
const tabRow = f => f.cells[0].map(c => c.symbol).join('');
const tabObserve = f => {
  const newTitle = visibleMatches(f, '+ New session').filter(p => p.y === 0);
  // The promoted Home slot also starts with " + "; it is not an add button.
  const add = visibleMatches(f, ' + ').filter(p => p.y === 0 &&
    !newTitle.some(title => title.x === p.x+1));
  const old = visibleMatches(f, oldTitlePrefix).filter(p => p.y === 0);
  return {add, old, new_title:newTitle, old_content:f.text.includes('GEOMETRY-SHORT: tool read completed.'),
    home_prompt:f.text.includes('Reader · MiMo-V2.6-Flash Free') && f.text.includes('Ask anything'),
    header:tabRow(f).trimEnd()};
};
const secondTitle = 'Second fixture session';
const renamedTitle = 'Paired renamed session';
const regeneratedTitle = 'Regenerated fixture title';
const restartObserve = f => ({...tabObserve(f), second:visibleMatches(f,secondTitle.slice(0,6)).filter(p=>p.y===0),
  second_content:f.text.includes('GEOMETRY-SECOND: tool read completed.')});
try {
  browser = await chromium.launch({headless: true, env: cleanEnv});
  const profile = {frontend: '@xterm/xterm', frontend_version: require('@xterm/xterm/package.json').version,
    vt_parser: 'xterm.js 6.0.0', chromium_version: browser.version(), playwright_version: require('playwright/package.json').version,
    font_family: 'DejaVu Sans Mono', font_match: execute(['fc-match','-f','%{family}|%{style}|%{file}|%{fontversion}', 'DejaVu Sans Mono']).stdout,
    fallback_fonts: null,
    font_size: 14, device_scale_factor: 1, dpi: 96, padding: 0, opacity: 1, ligatures: false,
    columns: Number(args.columns || 160), rows: Number(args.rows || 48), TERM: 'xterm-256color', COLORTERM: 'truecolor', locale: 'C.UTF-8',
    unicode_width_policy: '@xterm/addon-unicode11 0.9.0 (Unicode 11)',
    settings: {theme: 'opencode', mode: 'dark', sidebar: args.sidebar || 'auto', devtools: args.devtools === 'unset' ? null : args.devtools === 'true', tabs: args.tabs || 'horizontal',
      clock_policy: 'real application wall clock; fixed provider created_at; no masking or clock claim',
        animations: scanner ? scannerAnimation : 'original supported animations=false; completed states only; terminal cursorBlink=false',
       ...(scanner ? {scanner_cancel:scannerCancel} : {}),
       ...(modelsInteraction ? {models_interaction:true,catalog_extension:'fixture-scroll-00..11'} : {})}};
  for (const origin of ['upstream','oc']) {
    profile.columns = Number(args.columns || 160); profile.rows = Number(args.rows || 48);
    const binary = args[origin === 'upstream' ? 'reference' : 'oc'];
    if (!binary) { lock.attempts.push({origin, status: 'SKIPPED', reason: 'No explicit binary supplied'}); continue; }
    if (!path.isAbsolute(binary)) throw Error('Binary must be an explicit absolute path');
    const hash = sha(fs.readFileSync(binary));
    if (origin === 'upstream' && hash !== '2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a') throw Error('Reference binary SHA mismatch');
    const dir = path.join(output, origin);
    fs.mkdirSync(dir);
    const spec = {binary, origin, columns: profile.columns, rows: profile.rows, isolated_root: isolated, fixture,
      sample: args.sample || 'table', sidebar: profile.settings.sidebar, devtools: profile.settings.devtools,
      tabs: profile.settings.tabs, variants: args.variants === 'true', startup_error: args['startup-error'] === 'true',
      agent_profile: args['agent-profile'] === 'true',
       seed_root: args['seed-root'], session: args.session, tab_restart: tabRestart || renameSession,
         regenerate_title: regenerateTitle, two_turn:twoTurn, models_interaction:modelsInteraction,
        ...(scanner ? {scanner:true, animations:scannerAnimation} : {})};
    fs.writeFileSync(path.join(dir,'bridge-spec.json'), JSON.stringify(spec, null, 2));
    lock[origin] = {...lock[origin], executable_path: binary, executable_sha256: hash};
    const page = await browser.newPage({viewport: {width: 1800, height: 1100}, deviceScaleFactor: 1});
    await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
    await page.addStyleTag({path: path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
    await page.addScriptTag({path: path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
    await page.addScriptTag({path: path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
    await page.addScriptTag({path: path.join(here,'frontend.js')});
    const child = spawn('/usr/bin/python3', [path.join(here,'bridge.py'), path.join(dir,'bridge-spec.json')], {env: cleanEnv, stdio: ['pipe','pipe','pipe']});
    const logs = [], chunks = [[],[]], inputs = [];
    let generation=0, prequitBoundary;
    let writeQueue = Promise.resolve(), bridgeExit, bridgeError = '';
    const closed = new Promise(r => child.once('close',r));
    child.stderr.on('data', b => { bridgeError += b.toString(); });
    child.on('exit', c => { bridgeExit = c; });
    const send = (data, kind='terminal_reply') => {
      inputs.push({at_ms: Date.now(), kind, base64: Buffer.from(data).toString('base64')});
      fs.writeFileSync(path.join(dir,'inputs.json'),JSON.stringify(inputs,null,2)+'\n');
      if (!child.stdin.destroyed) child.stdin.write(JSON.stringify({kind:'input',data:Buffer.from(data).toString('base64')})+'\n');
    };
    await page.exposeFunction('terminalReply', d => send(d));
    await page.evaluate(p => startTerminal(p), profile);
    readline.createInterface({input: child.stdout}).on('line', line => {
      const event = JSON.parse(line);
      if (event.kind === 'output') {
        const bytes=Buffer.from(event.data, 'base64');
        const n=event.generation || 0;
        chunks[n].push(bytes);
        // Keep a diagnostic VT prefix if the capture process is interrupted
        // before the normal per-generation teardown writes its sealed copy.
        fs.appendFileSync(path.join(dir,'raw.vt'),bytes);
        if(n===generation) writeQueue = writeQueue.then(() => page.evaluate(d => writeTerminal(d), event.data));
      } else { logs.push(event); fs.writeFileSync(path.join(dir,'protocol.json'),JSON.stringify(logs,null,2)+'\n'); }
    });
    const frame = async () => {await writeQueue; return page.evaluate(() => readTerminal());};
    const waitFor = async (predicate, label, timeout=45000) => {
      const deadline = Date.now()+timeout;
      let previous, stable=0;
      while (Date.now()<deadline) {
        const f = await frame();
        const key = sha(JSON.stringify(f));
        stable = key===previous ? stable+1 : 0; previous=key;
        if (predicate(f) && stable >= 4) return f;
        if (bridgeExit !== undefined) throw Error('Bridge exited '+bridgeExit+' waiting for '+label);
        await sleep(200);
      }
      throw Error('Timed out waiting for '+label);
    };
     const capture = async (scenario, f, status, scannerSignature) => {
      const name = path.join(dir,scenario);
      if ((scanner && ['.cells.json','.txt','.png','.render.json','.vt'].some(ext=>fs.existsSync(name+ext))) ||
          fs.existsSync(name+'.cells.json')) throw Error('Refusing to overwrite scenario: '+name);
      // Repaint the same VT buffer on both sides to distinguish a stale xterm
      // DOM row left behind by resize from a real application cell mismatch.
      // This never changes the buffer, input sequence, or comparator regions.
      const refreshed = args['refresh-before-capture'] === 'true';
      if (refreshed) await page.evaluate(() => term.refresh(0,term.rows-1));
      // The grid can settle before Chromium paints resized canvas/text layers.
       await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
       const paintWaitFrames = 2;
       const rect = await page.locator('.xterm-screen').boundingBox();
       if (!rect) throw Error('Missing .xterm-screen screenshot bounds');
       const clip = {x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)};
       // For scanner stages the child has acknowledged SIGSTOP. Verify the
       // complete styled grid before and after painting, never select a later
       // animation phase to satisfy a previously observed one.
       if(scannerSignature && scannerSignature.read(f)!==scannerSignature.expected)
         throw Error('Scanner phase changed before '+scenario+' screenshot');
       if(scannerSignature && sha(JSON.stringify(await frame()))!==sha(JSON.stringify(f)))
         throw Error('Paused scanner grid changed before '+scenario+' screenshot');
       const before=await page.evaluate(() => readCaptureGeometry());
       const png=await page.screenshot({path:name+'.png', clip});
       const after=await page.evaluate(() => readCaptureGeometry());
       const afterFrame=await frame();
       // Profile records requested shared inputs only. Measured CSS/PNG facts are
       // per-side observations and must not alter the paired environment ID.
       const environment = sha(canonical(profile));
      const {text, ...grid} = f;
      fs.writeFileSync(name+'.cells.json', JSON.stringify({schema_version:1, origin, scenario,
        fixture_sha256:fixtureSha, environment_id:environment,
        producer_commit: origin==='upstream' ? lock.sources.upstream_commit : commit, ...grid}));
      fs.writeFileSync(name+'.txt', text+'\n');
       const render = {schema_version:1, origin, scenario, environment_id:environment,
         terminal_refresh_from_buffer:refreshed,
         paint_wait_request_animation_frames:paintWaitFrames,
         ...(scannerSignature ? {paused_scanner_signature:scannerSignature.expected,
           paused_grid_stable:sha(JSON.stringify(afterFrame))===sha(JSON.stringify(f))} : {}), before, after,
        layout_changed_during_screenshot:canonical(before)!==canonical(after),
        screenshot: {clip, png_width:png.readUInt32BE(16), png_height:png.readUInt32BE(20)}};
      fs.writeFileSync(name+'.render.json',JSON.stringify(render,null,2)+'\n');
       if(sha(JSON.stringify(afterFrame)) !== sha(JSON.stringify(f)) ||
           (scannerSignature && scannerSignature.read(afterFrame)!==scannerSignature.expected)) {
          status='UNSTABLE_CAPTURE'; result=1;
          lock.attempts.push({origin,scenario,status,reason:'VT grid changed during PNG capture'});
      }
      fs.writeFileSync(name+'.vt', Buffer.concat(chunks[generation]));
      lock.captures.push({origin,scenario,status,environment_id:environment,
        cells_sha256:sha(fs.readFileSync(name+'.cells.json')),png_sha256:sha(fs.readFileSync(name+'.png')),
        render_sha256:sha(fs.readFileSync(name+'.render.json')),
        path:path.relative(output,name)});
      lock.profile=profile; json('capture.lock.json',lock);
      return status;
    };
    try {
      if(args['startup-error'] === 'true' || args['seed-root']) {
        const expected = args['startup-error'] === 'true' || args.session === 'foreign' ? 'Native startup error' : origin==='upstream' ? 'Geometry parent' : 'Geometry child';
        const f = await waitFor(f => f.text.includes(expected) && (origin!=='upstream' || f.text.includes('Subagents')), 'native route / original child composer');
        if(f.text.includes('DO-NOT-LEAK-KEY')) throw Error('unsafe config detail reached terminal');
        if(f.text.includes('Context')) throw Error('unexpected sidebar on child/error route');
        const rightBackground = f.cells[10].at(-1).bg;
        if(rightBackground!=='#0a0a0a') throw Error('unexpected styled sidebar boundary on child/error route');
        fs.writeFileSync(path.join(dir,'chrome-checks.json'),JSON.stringify({route:args.session || 'preflight',right_background:rightBackground,sidebar_absent:true,expected_marker:expected},null,2)+'\n');
        await capture(args['startup-error'] === 'true' ? 'preflight-error' : 'session-'+args.session, f, 'CAPTURED_NATIVE_CAPABILITY');
        lock.attempts.push({origin,status:origin==='oc'?'EXECUTED_NATIVE_ROUTE':'EXECUTED_ORIGINAL_CHILD_ROUTE'});
        continue;
      }
      const initial = await waitFor(f => /Build|Untitled session|MiMo-V2.6-Flash Free/.test(f.text), 'initial prompt');
      if(spec.agent_profile && !initial.text.includes('Reader · MiMo-V2.6-Flash Free'))
        throw Error('Explicit paired profile is not selected in the initial prompt');
      if(args.geometry === 'true') await capture('home', initial, 'CAPTURED');
      const autocompleteChecks=[];
      const providerCounts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
        completed:logs.filter(e=>e.kind==='provider_completed').length,
        invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
      const ctrlCChecks=[];
      const probeCtrlC=async route=>{
        const baseline=providerCounts();
        const prefix=`interrupt-draft-${route}-`;
        const chip='[Pasted ~3 lines]';
        const observe=f=>{
          const row=f.cells[f.cursor.y]?.map(c=>c.symbol).join('') ?? '';
          const start=row.indexOf('┃  ');
          return {prefix:visibleMatches(f,prefix),chip:visibleMatches(f,chip),
            draft_start:start<0?null:start+3,draft:start<0?null:row.slice(start+3).trimEnd(),
            commands:visibleMatches(f,'Commands'),modal_query:visibleMatches(f,'ctrlcmodal'),
            cursor:f.cursor,provider_counts:providerCounts(),bridge_exit:bridgeExit,
            exit_events:logs.filter(e=>e.kind==='exit')};
        };
        const empty=o=>o.draft_start!==null && o.cursor.x===o.draft_start &&
          (o.draft==='' || o.draft.startsWith('Ask anything…'));
        const unchanged=()=>canonical(providerCounts())===canonical(baseline);
        lock.ctrl_c ??= {};
        lock.ctrl_c[origin] ??= {status:'IN_PROGRESS',checks:ctrlCChecks};
        const save=()=>{
          fs.writeFileSync(path.join(dir,'ctrl-c-checks.json'),JSON.stringify({checks:ctrlCChecks},null,2)+'\n');
          json('capture.lock.json',lock);
        };
        const record=(stage,f,predicates,extra={})=>{
          ctrlCChecks.push({route,stage,baseline,observed:observe(f),predicates,...extra,
            grid_sha256:sha(JSON.stringify(f))});
          save();
          if(Object.values(predicates).some(v=>v!==true)) throw Error('Ctrl+C predicate failed: '+route+' '+stage);
        };
        const shot=async(stage,f)=>{
          const status=await capture(`ctrl-c-${route}-${stage}`,f,'CAPTURED_CTRL_C');
          record(stage+'-capture',await frame(),{stable_capture:status==='CAPTURED_CTRL_C',
            no_provider_request:unchanged()});
        };
        // The literal prefix makes the draft visible even when the pasted
        // three-line payload is rendered as a compact paste chip.
        send(prefix,`ctrl_c_${route}_draft_prefix`);
        send('\x1b[200~line-one\nline-two\nline-three\x1b[201~',`ctrl_c_${route}_multiline_paste`);
        const before=await waitFor(f=>{const o=observe(f);
          return o.prefix.length===1 && o.chip.length===1 && unchanged() && bridgeExit===undefined;
        },`ctrl-c ${route} multiline draft`,12000);
        record('draft-before',before,{prefix_visible:observe(before).prefix.length===1,
          multiline_chip_visible:observe(before).chip.length===1,pty_alive:bridgeExit===undefined,
          no_provider_request:unchanged()});
        await shot('draft-before',before);
        if(route==='session') {
          send('\x10','ctrl_c_session_commands_open');
          const opened=await waitFor(f=>observe(f).commands.length===1 && observe(f).prefix.length===1,
            'Commands over retained root draft',12000);
          record('modal-open',opened,{commands_open:observe(opened).commands.length===1,
            root_draft_retained:observe(opened).prefix.length===1,no_provider_request:unchanged()});
          send('ctrlcmodal','ctrl_c_session_modal_query');
          const queried=await waitFor(f=>{const o=observe(f);
            return o.commands.length===1 && o.modal_query.length===1 && o.prefix.length===1 && unchanged();
          },'focused Commands query',12000);
          record('modal-query-before',queried,{commands_open:true,query_visible:true,
            root_draft_retained:true,no_provider_request:unchanged()});
          await shot('modal-query-before',queried);
          send('\x03','ctrl_c_session_modal_interrupt');
          const afterModal=await waitFor(f=>{const o=observe(f);
            return o.prefix.length===1 && o.modal_query.length===0 && unchanged() && bridgeExit===undefined;
          },'modal Ctrl+C cleared query or closed modal',12000);
          const modalOpen=observe(afterModal).commands.length===1;
          record('modal-after',afterModal,{query_absent:observe(afterModal).modal_query.length===0,
            root_draft_retained:observe(afterModal).prefix.length===1,pty_alive:bridgeExit===undefined,
            no_provider_request:unchanged()},{modal_outcome:modalOpen?'query_cleared':'dismissed'});
          await shot('modal-after',afterModal);
          if(modalOpen) {
            send('\x1b','ctrl_c_session_modal_escape');
            const dismissed=await waitFor(f=>observe(f).commands.length===0 && observe(f).prefix.length===1 && unchanged(),
              'dismiss Commands without clearing root draft',12000);
            record('modal-dismissed',dismissed,{commands_closed:true,root_draft_retained:true,
              no_provider_request:unchanged()});
          }
        }
        const justBefore=await frame();
        record('before-root-interrupt',justBefore,{root_draft_visible:observe(justBefore).prefix.length===1,
          multiline_chip_visible:observe(justBefore).chip.length===1,
          modal_closed:observe(justBefore).commands.length===0,no_provider_request:unchanged(),
          pty_alive:bridgeExit===undefined});
        send('\x03',`ctrl_c_${route}_root_nonempty`);
        const cleared=await waitFor(f=>{const o=observe(f);
          return o.prefix.length===0 && o.chip.length===0 && o.commands.length===0 &&
            empty(o) && unchanged() && bridgeExit===undefined;
        },`ctrl-c ${route} cleared root draft`,12000);
        record('root-cleared',cleared,{draft_prefix_absent:observe(cleared).prefix.length===0,
          multiline_chip_absent:observe(cleared).chip.length===0,
          empty_prompt_visible:empty(observe(cleared)),
          pty_alive:bridgeExit===undefined,no_provider_request:unchanged()});
        await shot('root-cleared',cleared);
        if(route==='session') {
          const beforeExitBytes=Buffer.concat(chunks[generation]).length;
          send('\x03','ctrl_c_session_root_empty_exit');
          const deadline=Date.now()+15000;
          while(!logs.some(e=>e.kind==='exit' && e.generation===0) && Date.now()<deadline) await sleep(100);
          const exit=logs.find(e=>e.kind==='exit' && e.generation===0);
          const exitBytes=Buffer.concat(chunks[generation]).subarray(beforeExitBytes).toString('latin1');
          const alternateScreenLeft=/\x1b\[\?(?:1049|1047|47)l/.test(exitBytes);
          const cursorRestored=/\x1b\[\?25h/.test(exitBytes);
          record('empty-root-exit',cleared,{natural_exit:exit?.termination==='natural',
            zero_exit_code:exit?.code===0,alternate_screen_left:alternateScreenLeft,
            cursor_restored:cursorRestored,no_provider_request:unchanged()},
          {exit,exit_vt_sha256:sha(Buffer.from(exitBytes,'latin1')),
            exit_vt_byte_length:Buffer.byteLength(exitBytes,'latin1')});
        }
      };
      const autocompleteObserve=f=>{
        const rows=f.cells.map(row=>row.map(c=>c.symbol).join(''));
        const draftRow=rows[f.cursor.y] || '';
        const draftStart=draftRow.indexOf('┃  ');
        const menu=rows.map((text,y)=>({y,text:text.trimEnd()})).filter(r=>
          r.y<f.cursor.y && r.y>=f.cursor.y-12 && /┃\s+\/\S/.test(r.text));
        return {draft: draftStart<0?null:draftRow.slice(draftStart+3).trimEnd(),
          draft_x:draftStart<0?null:draftStart+3,menu,
          rename_options:menu.filter(r=>r.text.includes('/rename')),
          reload_notice:f.text.includes('Configuration reloaded'),
          cursor:f.cursor,provider_counts:providerCounts()};
      };
       const autocompleteKeysChecks=[];
       const probeAutocompleteKeys=async route=>{
         const baseline=providerCounts();
         // Selection is a styled row, not a label or menu-order guess. Sample
         // the slash cell's background on each actual painted option row.
         const selection=(f,focusedBg)=>{
           const options=autocompleteObserve(f).menu.map(({y,text})=>{
             const label=/\/\S+/.exec(text.slice(text.indexOf('┃')+1))?.[0];
             const x=label===undefined?-1:text.indexOf(label);
             return {y,label,x,fg:f.cells[y]?.[x]?.fg,bg:f.cells[y]?.[x]?.bg};
           });
           const selected=options.filter(row=>row.bg===focusedBg);
           return {options,selected_index:selected.length===1?options.indexOf(selected[0]):null,
             selected_option:selected.length===1?selected[0]:null};
         };
         const save=()=>{
           fs.writeFileSync(path.join(dir,'autocomplete-keys-checks.json'),JSON.stringify({checks:autocompleteKeysChecks},null,2)+'\n');
           json('capture.lock.json',lock);
         };
         const empty=o=>o.cursor.x===o.draft_x && (o.draft==='' || o.draft?.startsWith('Ask anything…'));
         const noRequests=()=>canonical(providerCounts())===canonical(baseline);
         const check=(stage,f,predicates,details={})=>{
           autocompleteKeysChecks.push({route,stage,baseline,observed:autocompleteObserve(f),predicates,
             ...details,grid_sha256:sha(JSON.stringify(f))});
           save();
           if(Object.values(predicates).some(value=>value!==true))
             throw Error('Autocomplete keyboard predicate failed: '+route+' '+stage);
         };
         const awaitState=async(stage,predicate)=>{
           try {return await waitFor(predicate,`autocomplete keys ${route} ${stage}`,12000);}
           catch(e) {
             autocompleteKeysChecks.push({route,stage,baseline,observed:autocompleteObserve(await frame()),
               predicates:{state_reached:false},reason:e.message});
             save();
             throw e;
           }
         };
         const shot=async(stage,f)=>{
           if(await capture(`autocomplete-keys-${route}-${stage}`,f,'CAPTURED_AUTOCOMPLETE_KEYS')!=='CAPTURED_AUTOCOMPLETE_KEYS')
             throw Error('Unstable autocomplete keyboard '+route+' '+stage);
         };
         // Exact /reload + Enter tests command execution, not fuzzy /ren + Tab.
         // Keep both routes' actual suggestions; never equate their inventories.
         send('/reload',`autocomplete_keys_${route}_reload_type`);
         const beforeEnter=await awaitState('before-enter',f=>{const o=autocompleteObserve(f);
           return o.draft==='/reload' && o.menu.some(r=>/┃\s+\/reload(?:\s|┃)/.test(r.text)) &&
             !o.reload_notice && noRequests();});
         const ready=autocompleteObserve(beforeEnter);
         check('before-enter',beforeEnter,{draft_visible:ready.draft==='/reload',
           reload_option_visible:ready.menu.some(r=>/┃\s+\/reload(?:\s|┃)/.test(r.text)),
           prior_reload_notice_absent:!ready.reload_notice,no_provider_request:noRequests()});
         await shot('before-enter',beforeEnter);
         send('\r',`autocomplete_keys_${route}_reload_enter`);
         const afterEnter=await awaitState('after-enter',f=>{const o=autocompleteObserve(f);
           return o.reload_notice && empty(o) && o.menu.length===0 && noRequests();});
         const reloaded=autocompleteObserve(afterEnter);
         check('after-enter',afterEnter,{reload_success_visible:reloaded.reload_notice,
           draft_cleared:empty(reloaded),menu_hidden:reloaded.menu.length===0,no_provider_request:noRequests()});
         await shot('after-enter',afterEnter);
         // The /reload success toast lasts 5s. Wait for its actual disappearance
         // before taking the /ren screenshot so the captured grid cannot change
         // solely because that toast expires during PNG capture.
         const expiredNotice=await awaitState('reload-notice-expired',f=>{const o=autocompleteObserve(f);
           return !o.reload_notice && empty(o) && o.menu.length===0 && noRequests();});
         const expired=autocompleteObserve(expiredNotice);
         check('reload-notice-expired',expiredNotice,{reload_notice_absent:!expired.reload_notice,
           draft_cleared:empty(expired),menu_hidden:expired.menu.length===0,no_provider_request:noRequests()});
         send('/ren',`autocomplete_keys_${route}_ren_type`);
         const beforeEsc=await awaitState('before-esc',f=>{const o=autocompleteObserve(f);
           return o.draft==='/ren' && o.menu.length>0 && noRequests();});
         const ren=autocompleteObserve(beforeEsc);
         check('before-esc',beforeEsc,{draft_visible:ren.draft==='/ren',
           menu_visible:ren.menu.length>0,no_provider_request:noRequests()});
         await shot('before-esc',beforeEsc);
         if(autocompleteKeysMove) {
           const focusedBg=selection(beforeEsc).options[0]?.bg;
           const initial=selection(beforeEsc,focusedBg);
           check('movement-initial',beforeEsc,{multiple_options:initial.options.length>1,
             unique_styled_selection:initial.selected_index===0,
             distinct_unselected_background:initial.options.slice(1).every(o=>o.bg!==focusedBg),
             draft_unchanged:ren.draft==='/ren',no_provider_request:noRequests()},
           {selection:initial});
           // Pinned keybind.ts:270-271; autocomplete.tsx:584-587 wraps.
           // before-esc is the initial movement frame, already captured above.
           const steps=[['up','\x1b[A',initial.options.length-1],
             ['ctrl-p','\x10',initial.options.length-2],
             ['down','\x1b[B',initial.options.length-1],['ctrl-n','\x0e',0]];
           let previous=initial.selected_index;
           const labels=initial.options.map(o=>o.label);
           for(const [name,key,expected] of steps) {
             send(key,`autocomplete_keys_${route}_movement_${name}`);
             const stage=`movement-${name}`;
             const moved=await awaitState(stage,f=>{
               const o=autocompleteObserve(f), s=selection(f,focusedBg);
               return o.draft==='/ren' && noRequests() &&
                 canonical(s.options.map(row=>row.label))===canonical(labels) &&
                 s.selected_index!==null && s.selected_index!==previous;
             });
             const s=selection(moved,focusedBg);
             await shot(stage,moved);
             check(stage,moved,{selected_index:s.selected_index===expected,
               selected_option:s.selected_option?.label===labels[expected],
               draft_unchanged:autocompleteObserve(moved).draft==='/ren',
               no_provider_request:noRequests()},{selection:s,expected_index:expected});
             previous=s.selected_index;
           }
         }
         send('\x1b',`autocomplete_keys_${route}_ren_escape`);
         const afterEsc=await awaitState('after-esc',f=>{const o=autocompleteObserve(f);
           return o.draft==='/ren' && o.menu.length===0 && noRequests();});
         const escaped=autocompleteObserve(afterEsc);
         check('after-esc',afterEsc,{draft_retained:escaped.draft==='/ren',
           menu_hidden:escaped.menu.length===0,no_provider_request:noRequests()});
         await shot('after-esc',afterEsc);
         send('\x7f'.repeat(32),`autocomplete_keys_${route}_ren_clear`);
         const cleared=await awaitState('cleared',f=>{const o=autocompleteObserve(f);
           return empty(o) && o.menu.length===0 && noRequests();});
         const reset=autocompleteObserve(cleared);
         check('cleared',cleared,{draft_cleared:empty(reset),menu_hidden:reset.menu.length===0,
           no_provider_request:noRequests()});
         if(route==='session' && autocompleteKeysRename) {
           send('/rename',`autocomplete_keys_${route}_rename_type`);
           const beforeRename=await awaitState('rename-before-enter',f=>{const o=autocompleteObserve(f);
             return o.draft==='/rename' && o.menu.length>0 && noRequests();});
           const rename=autocompleteObserve(beforeRename);
           check('rename-before-enter',beforeRename,{draft_visible:rename.draft==='/rename',
             menu_visible:rename.menu.length>0,no_provider_request:noRequests()});
           await shot('rename-before-enter',beforeRename);
           send('\r',`autocomplete_keys_${route}_rename_enter`);
           const afterRename=await awaitState('rename-after-enter',f=>{const o=autocompleteObserve(f);
             return o.draft==='/rename' && o.cursor.x===o.draft_x+'/rename '.length &&
               o.menu.length===0 && noRequests();});
           const inserted=autocompleteObserve(afterRename);
           check('rename-after-enter',afterRename,{command_with_space_inserted:inserted.draft==='/rename' &&
             inserted.cursor.x===inserted.draft_x+'/rename '.length,
             menu_hidden:inserted.menu.length===0,no_provider_request:noRequests()});
           await shot('rename-after-enter',afterRename);
           send('\x7f'.repeat(32),`autocomplete_keys_${route}_rename_clear`);
           const afterClear=await awaitState('rename-cleared',f=>{const o=autocompleteObserve(f);
             return empty(o) && o.menu.length===0 && noRequests();});
           const final=autocompleteObserve(afterClear);
           check('rename-cleared',afterClear,{draft_cleared:empty(final),menu_hidden:final.menu.length===0,
             no_provider_request:noRequests()});
         }
       };
      const probeAutocomplete=async route=>{
        const baseline=providerCounts();
        const states=[['trigger','/'],['filtered','ren'],['after-tab','\t']];
        for(const [stage,key] of states) {
          send(key,`autocomplete_${route}_${stage}`);
          // Record what the application actually paints; no injected menu text
          // and no assumption that Home has the session-only /rename action.
          await sleep(300);
          const f=await waitFor(()=>true,`autocomplete ${route} ${stage}`,7000);
          const observed=autocompleteObserve(f);
          const previous=autocompleteChecks.at(-1);
          const checks={route,stage,typed_query:stage==='trigger'?'/':stage==='filtered'?'/ren':null,
            observed,predicates:{typed_query_visible:stage==='after-tab'?null:
              observed.draft===(stage==='trigger'?'/':'/ren'),
              menu_visible:observed.menu.length>0,
              rename_option_visible:observed.rename_options.length>0,
              tab_changed_grid:stage==='after-tab'?sha(JSON.stringify(f))!==previous?.grid_sha256:null,
              no_provider_request:canonical(providerCounts())===canonical(baseline)},
            grid_sha256:sha(JSON.stringify(f))};
          autocompleteChecks.push(checks);
          fs.writeFileSync(path.join(dir,'autocomplete-checks.json'),JSON.stringify(autocompleteChecks,null,2)+'\n');
          if(await capture(`autocomplete-${route}-${stage}`,f,'CAPTURED_AUTOCOMPLETE_DIAGNOSTIC')!=='CAPTURED_AUTOCOMPLETE_DIAGNOSTIC')
            throw Error('Unstable autocomplete '+route+' '+stage);
        }
      };
      const mentionChecks=[];
      const mentionObserve=f=>{
        const rows=f.cells.map(row=>row.map(c=>c.symbol).join(''));
        const draftRow=rows[f.cursor.y] || '';
        const draftStart=draftRow.indexOf('┃  ');
        // Read only painted cells. The reference may label file suggestions
        // differently; keep actual rows and predicates instead of synthesizing
        // a matching menu or a completed path.
        const menu=rows.map((text,y)=>({y,text:text.trimEnd()})).filter(r=>
          r.y<f.cursor.y && r.y>=f.cursor.y-12 && r.text.includes('┃') &&
          /┃\s*\S/.test(r.text));
        const draft=draftStart<0?null:draftRow.slice(draftStart+3).trimEnd();
        return {draft,draft_x:draftStart<0?null:draftStart+3,menu,
          fixture_rows:menu.filter(r=>r.text.includes('fixture-note.txt')),
          cursor:f.cursor,provider_counts:providerCounts()};
      };
      const probeMention=async route=>{
        const baseline=providerCounts();
        const states=[['trigger','@'],['filtered','fixture'],['after-tab','\t']];
        for(const [stage,key] of states) {
          send(key,`mention_${route}_${stage}`);
          await sleep(300);
          const f=await waitFor(()=>true,`mention ${route} ${stage}`,7000);
          const observed=mentionObserve(f);
          const previous=mentionChecks.at(-1);
          const checks={route,stage,typed_query:stage==='trigger'?'@':stage==='filtered'?'@fixture':null,
            observed,predicates:{typed_query_visible:stage==='after-tab'?null:
              observed.draft===(stage==='trigger'?'@':'@fixture'),
              menu_visible:observed.menu.length>0,
              fixture_option_visible:observed.fixture_rows.length>0,
              relative_mention_inserted:stage==='after-tab'?observed.draft==='@fixture-note.txt':null,
              tab_changed_grid:stage==='after-tab'?sha(JSON.stringify(f))!==previous?.grid_sha256:null,
              no_provider_request:canonical(providerCounts())===canonical(baseline)},
            grid_sha256:sha(JSON.stringify(f))};
          mentionChecks.push(checks);
          fs.writeFileSync(path.join(dir,'mention-checks.json'),JSON.stringify(mentionChecks,null,2)+'\n');
          if(await capture(`mention-${route}-${stage}`,f,'CAPTURED_MENTION_DIAGNOSTIC')!=='CAPTURED_MENTION_DIAGNOSTIC')
            throw Error('Unstable mention '+route+' '+stage);
          if(!checks.predicates.no_provider_request) throw Error('Unexpected provider request during mention '+route+' '+stage);
        }
        // Tab may insert a path of varying length on either side. Backspace
        // never submits the draft; assert the empty prompt before proceeding.
        send('\x7f'.repeat(64),`mention_${route}_clear_draft`);
        const cleared=await waitFor(f=>{const o=mentionObserve(f);
          return o.cursor.x===o.draft_x && (o.draft==='' || o.draft?.startsWith('Ask anything…')) && o.menu.length===0;
        },`cleared ${route} mention draft`,7000);
        const observed=mentionObserve(cleared);
        const predicates={draft_cleared:observed.cursor.x===observed.draft_x &&
          (observed.draft==='' || observed.draft?.startsWith('Ask anything…')) && observed.menu.length===0,
          no_provider_request:canonical(providerCounts())===canonical(baseline)};
        mentionChecks.push({route,stage:'cleared',observed,predicates});
        fs.writeFileSync(path.join(dir,'mention-checks.json'),JSON.stringify(mentionChecks,null,2)+'\n');
        if(!predicates.no_provider_request) throw Error('Unexpected provider request while clearing '+route+' mention');
      };
      if(autocomplete) {
        lock.autocomplete ??= {};
        lock.autocomplete[origin]={status:'IN_PROGRESS',checks:autocompleteChecks};
        await probeAutocomplete('home');
        // Undo only the test draft, never submit a slash command from Home.
        send('\x7f'.repeat(40),'autocomplete_home_clear_draft');
        const cleared=await waitFor(f=>{const o=autocompleteObserve(f);
          return o.cursor.x===o.draft_x && o.menu.length===0 && f.text.includes('Ask anything');
        },'cleared Home autocomplete draft',7000);
        autocompleteChecks.push({route:'home',stage:'cleared',observed:autocompleteObserve(cleared),
          predicates:{draft_cleared:true,no_provider_request:providerCounts().requests===0}});
        fs.writeFileSync(path.join(dir,'autocomplete-checks.json'),JSON.stringify(autocompleteChecks,null,2)+'\n');
      }
      if(autocompleteKeys) {
        lock.autocomplete_keys ??= {};
        lock.autocomplete_keys[origin]={status:'IN_PROGRESS',checks:autocompleteKeysChecks};
        await probeAutocompleteKeys('home');
      }
      if(ctrlC) await probeCtrlC('home');
      if(mention) {
        lock.mention ??= {};
        lock.mention[origin]={status:'IN_PROGRESS',checks:mentionChecks};
        await probeMention('home');
      }
       if(scanner) {
         const stages = scannerAnimation ? ['forward','end-hold','reverse','start-hold'] : ['fallback'];
          const observations=[], checks={animation:scannerAnimation, cancel:scannerCancel,
            observations, pause_attempts:[], stages:{}, predicates:{}, release:null};
         lock.scanner ??= {};
         lock.scanner[origin]={status:'IN_PROGRESS',checks};
         const save=()=>{fs.writeFileSync(path.join(dir,'scanner-checks.json'),JSON.stringify(checks,null,2)+'\n');json('capture.lock.json',lock);};
         // Locate the painted interrupt hint independently on each side. The
         // adjacent block run is read from VT styled cells, not a timer/DOM.
           const observe=f=>{
            const direct=visibleMatches(f,'esc interrupt');
            const again=visibleMatches(f,'esc again to interrupt');
            const hints=direct.length ? direct : again;
            if(hints.length!==1) return {hints,run_count:0,fallback:[],phase:null};
           const {x,y}=hints[0], row=f.cells[y];
           const cells=row.slice(x-9,x-1);
           const run=cells.length===8 && row[x-1]?.symbol===' ' &&
             cells.every(c=>c.width===1 && (c.symbol==='■' || c.symbol==='⬝')) ? {x:x-9,y,cells} : null;
           const active=run ? run.cells.flatMap((c,i)=>c.symbol==='■'?[i]:[]) : [];
           const fallback=visibleMatches(f,'[⋯]').filter(p=>p.y===y && p.x===x-4);
              return {hints,hint_text:direct.length?'esc interrupt':'esc again to interrupt',run,run_count:run?1:0,
             active,first:active[0]??null,last:active.at(-1)??null,fallback,
              signature:run?sha(JSON.stringify(run.cells)):null};
          };
          const phaseCells=(f,o)=>o.run?.cells ?? (o.fallback.length===1 ?
            f.cells[o.fallback[0].y].slice(o.fallback[0].x,o.fallback[0].x+3) : null);
          const indicatorAbsent=f=>observe(f).hints.length===0 &&
            !f.cells.slice(-3).some(row=>row.some(c=>c.symbol==='■' || c.symbol==='⬝')) &&
            visibleMatches(f,'[⋯]').length===0;
          const glyphs=cells=>cells?.map(c=>c.symbol).join('') ?? null;
          const colorDistance=(left,right)=>left.reduce((sum,c,i)=>sum+
            ['fg','bg'].reduce((n,key)=>n+[1,3,5].reduce((rgb,p)=>rgb+
              Math.abs(parseInt(c[key].slice(p,p+2),16)-parseInt(right[i][key].slice(p,p+2),16)),0),0),0);
          const referenceStages=lock.scanner?.upstream?.checks.stages;
          // The footer probe builds cells in a different property order from
          // readTerminal; hash the same styled fields in the probe's order.
          const referenceSignatures=origin==='oc' && scannerAnimation ? Object.fromEntries(
            stages.filter(stage=>referenceStages?.[stage]?.indicator_cells).map(stage=>[
              stage,sha(JSON.stringify(referenceStages[stage].indicator_cells.map(
                ({symbol,width,fg,bg,modifiers})=>({symbol,width,fg,bg,modifiers}))))])) : {};
          // Poll the real xterm VT footer cheaply while the child runs. A full
          // 120x40 styled read costs several 40ms animation frames; after ACK,
          // the capture still reads and compares the *entire* styled grid.
          const runningProbe=async()=>{
            await writeQueue;
            return page.evaluate(()=>{
              const buffer=term.buffer.active, colors=term._core._themeService.colors;
              const cells=Array.from({length:term.rows},()=>[]);
              for(let y=Math.max(0,term.rows-3);y<term.rows;y++) {
                const line=buffer.getLine(buffer.viewportY+y);
                const row=Array.from({length:term.cols},(_,x)=>{
                  const c=line.getCell(x);
                  return {symbol:c.getChars() || (c.getWidth()===0?'':' '),width:c.getWidth()};
                });
                cells[y]=row;
                const symbols=row.map(c=>c.symbol).join('');
                const hint=symbols.includes('esc interrupt')?'esc interrupt':'esc again to interrupt';
                const x=symbols.indexOf(hint);
                if(x<9 || symbols.indexOf(hint,x+1)!==-1) continue;
                const color=(c,fg)=>{
                  const value=fg?c.getFgColor():c.getBgColor();
                  const rgb=(fg?c.isFgRGB():c.isBgRGB())?value:
                    (fg?c.isFgPalette():c.isBgPalette())?colors.ansi[value].rgba>>>8:
                    (fg?colors.foreground:colors.background).rgba>>>8;
                  return '#'+rgb.toString(16).padStart(6,'0');
                };
                const modifiers=[['isBold','bold'],['isDim','dim'],['isItalic','italic'],
                  ['isUnderline','underlined'],['isBlink','slow_blink'],['isInverse','reversed'],
                  ['isInvisible','hidden'],['isStrikethrough','crossed_out']];
                for(let i=x-9;i<x-1;i++) {
                  const c=line.getCell(i);
                  row[i]={...row[i],fg:color(c,true),bg:color(c,false),
                    modifiers:modifiers.filter(([method])=>c[method]()).map(([,name])=>name).sort()};
                }
              }
              return {cells};
            });
          };
         const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
           completed:logs.filter(e=>e.kind==='provider_completed').length,
           invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
         const promptText=fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim();
         send('\x1b[200~'+promptText+'\x1b[201~','scanner_prompt_paste');
         await sleep(200);send('\r','scanner_submit');
          const started=Date.now();
          // The timer belongs to the HTTP handler, not the paused application.
           const deadline=started+(scannerAnimation?35000:8000);
           const candidateWindows={};
           const candidates={};
            let lastSignature=null, direction='forward', endBlank=false, endFade=false, startBlank=false, endMovingColor=null;
           let seenEndFade=false, seenStartFade=false, cycle=0;
          let requestId=0;
          const control=async kind=>{
            if(child.stdin.destroyed || bridgeExit!==undefined) throw Error('Scanner bridge unavailable for '+kind);
            const id=++requestId;
            child.stdin.write(JSON.stringify({kind,request_id:id})+'\n');
            const ackKind=kind==='pause_scanner'?'scanner_pause_ack':'scanner_resume_ack';
            const limit=Date.now()+4000;
            while(Date.now()<limit) {
              const ack=logs.find(e=>e.kind===ackKind && e.request_id===id && e.generation===generation);
              if(ack) {
                if(ack.error || ack.paused!==(kind==='pause_scanner')) throw Error('Scanner '+kind+' rejected: '+JSON.stringify(ack));
                return ack;
              }
              if(bridgeExit!==undefined) throw Error('Bridge exited waiting for '+ackKind);
              await sleep(10);
            }
            throw Error('Timed out waiting for '+ackKind+' '+id);
          };
          const forwardPositions=new Set(), reversePositions=new Set(), endColors=new Set(), startColors=new Set();
           const brightness=c=>/^#[0-9a-f]{6}$/i.test(c?.fg ?? '') ?
             [1,3,5].reduce((sum,i)=>sum+parseInt(c.fg.slice(i,i+2),16),0) : null;
           const uniformDots=o=>o.run && o.active.length===0 &&
             o.run.cells.every(c=>c.fg===o.run.cells[0].fg && /^#[0-9a-f]{6}$/i.test(c.fg));
           const endTrail=o=>o.run && o.first>0 && o.last===7 &&
             brightness(o.run.cells[o.first])<brightness(o.run.cells[7]);
           const reverseTrail=o=>o.run && o.first>0 && o.last===7 &&
             brightness(o.run.cells[o.first])>brightness(o.run.cells[7]);
           const forwardTrail=o=>o.run && o.first===0 && o.last>=1 && o.last<=6 &&
             brightness(o.run.cells[0])<brightness(o.run.cells[o.last]);
           // Moving edges have opposite color gradients. The identical all-dot
           // glyph rows are distinguished by their observed preceding edge and
           // the continuing fade, not by position in a wall-clock schedule.
           const stageMatches=(stage,o)=>o.hints.length===1 && (stage==='fallback' ?
             o.run_count===0 && o.fallback.length===1 : o.run_count===1 && (
               stage==='forward' ? forwardTrail(o) :
               stage==='end-hold' ? (endTrail(o) && o.run.cells[7].fg!==endMovingColor ||
                 uniformDots(o) && endMovingColor!==null &&
                 brightness(o.run.cells[0])<=brightness({fg:[...endColors].at(-1) ?? endMovingColor})) :
               stage==='reverse' ? reverseTrail(o) && o.first<=6 :
               stage==='start-hold' ? uniformDots(o) && startColors.size>=2 &&
                 brightness(o.run.cells[0])<=brightness({fg:[...startColors].at(-1)}) : false));
          try {
            while(Date.now()<deadline && stages.some(stage=>!checks.stages[stage])) {
               const f=await runningProbe(), o=observe(f);
             if(o.hints.length===1 && (o.signature || o.fallback.length===1) &&
                 (o.signature ?? 'fallback')!==lastSignature) {
               lastSignature=o.signature ?? 'fallback';
               const sample={elapsed_ms:Date.now()-started,at:new Date().toISOString(),cycle,
                 hint:o.hints[0],run:o.run,active:o.active,fallback:o.fallback,
                 provider_counts:counts(),signature:o.signature};
               observations.push(sample);
                const prior=direction;
                if(scannerAnimation && o.run) {
                  if(direction==='forward') {
                    if(o.first===0 && o.last!==null && o.last<7) forwardPositions.add(o.last);
                    if(o.last===7) {direction='end';endMovingColor=o.run.cells[7].fg;}
                  } else if(direction==='end') {
                    if(uniformDots(o)) {endBlank=true;endColors.add(o.run.cells[0].fg);}
                    if(endTrail(o) && o.run.cells[7].fg!==endMovingColor) {
                      endFade=true;endColors.add(o.run.cells[7].fg);
                    }
                    if(endBlank || endFade) seenEndFade=true;
                    // The short end hold can lie entirely between two costly
                    // browser samples. A reversed *color gradient* is still a
                    // real observed transition, even if no blank was sampled.
                    if((endBlank || endFade) && reverseTrail(o)) direction='reverse';
                  } else if(direction==='reverse') {
                    if(o.first>0 && o.last===7) reversePositions.add(o.first);
                    if(o.first===0 || (o.first===1 && o.last<7)) direction='start';
                  } else if(direction==='start') {
                    if(uniformDots(o)) {
                      startBlank=true;startColors.add(o.run.cells[0].fg);
                      if(startColors.size>=2) seenStartFade=true;
                    }
                    if(forwardTrail(o)) {
                      cycle++;direction='forward';endBlank=false;endFade=false;startBlank=false;
                      endMovingColor=null;endColors.clear();startColors.clear();
                      forwardPositions.add(o.last);
                    }
                  }
                 }
                sample.cycle=cycle;sample.transition={from:prior,to:direction};
                 // Classify original holds after their moving edge; native
                 // matching may instead use the exact reference signature.
              const expectedStage=stages.find(s=>!checks.stages[s]);
              const reference=origin==='oc' ? referenceStages?.[expectedStage]?.indicator_cells : null;
              const candidateCells=phaseCells(f,o);
              // A native exact styled-cell signature is enough to request a
              // pause now: the 40ms start hold may be gone before two distinct
              // start colors are sampled. Stage order and progression evidence
              // are still checked independently below.
              const exactLive=origin==='oc' && o.run_count===1 &&
                o.signature===referenceSignatures[expectedStage];
               const stage=exactLive || (origin==='upstream' || reference) &&
                 (!scannerAnimation || direction===({forward:'forward','end-hold':'end',reverse:'reverse','start-hold':'start'}[expectedStage])) &&
                  stageMatches(expectedStage,o) && (!reference || glyphs(candidateCells)===glyphs(reference)) ? expectedStage : null;
                 if(stage && !checks.stages[stage]) {
                  const attempt={stage, observed:sample, requested_at:new Date().toISOString()};
                  checks.pause_attempts.push(attempt);
                  try {
                    attempt.pause_ack=await control('pause_scanner');
                    const stable=await frame(), actual=observe(stable);
                    const actualCells=phaseCells(stable,actual);
                    attempt.paused={at:new Date().toISOString(),observed:actual,
                      grid_sha256:sha(JSON.stringify(stable))};save();
                    const exactPaused=reference && actual.hints.length===1 && actual.run_count===1 &&
                      canonical(actualCells)===canonical(reference);
                    const same=exactLive && exactPaused || stageMatches(stage,actual) &&
                      (!reference || glyphs(actualCells)===glyphs(reference));
                    attempt.paused.matches_requested_stage=Boolean(same);save();
                    if(same) {
                      // The paused *actual* phase supplies the exact signature;
                      // ACK latency is allowed to change the live candidate.
                      const signature={expected:actual.signature ?? 'fallback',
                        read:v=>observe(v).signature ?? (observe(v).fallback.length===1?'fallback':null)};
                      const peers=candidates[stage] ??= [];
                      const exact=reference && (exactPaused || canonical(actualCells)===canonical(reference));
                      // The comparator requires matching scenario fields. Save
                      // the first accepted paused match under its final name;
                      // never rename or rewrite a completed candidate capture.
                      const scenario='scanner-'+stage+(reference && !exact ? '-candidate-'+(peers.length+1) : '');
                      attempt.capture_name=scenario;
                      attempt.phase_match=reference ? exact?'EXACT_INDICATOR':'CANDIDATE' : 'REFERENCE';
                      const status=await capture(scenario,stable,'CAPTURED_SCANNER',
                        signature);
                      attempt.status=status;
                      const candidate={status,scenario,sample,paused:attempt.paused,
                        indicator_cells:actualCells,
                        color_distance:reference ? colorDistance(reference,actualCells) : 0};
                      peers.push(candidate);
                      if(!reference || exact) {
                        checks.stages[stage]={...candidate,phase_match:reference?'EXACT_INDICATOR':'REFERENCE'};
                      } else candidateWindows[stage] ??= Date.now();
                      save();
                    } else attempt.status='DIFFERENT_PHASE_AT_PAUSE';
                  } catch(e) {
                    attempt.status='FAILED';attempt.error=e.message;save();
                    throw e;
                  } finally {
                    // Also send resume when pause ACK is lost: the bridge may
                    // have stopped the child before the IPC event was observed.
                    if(child.stdin.destroyed || bridgeExit!==undefined) {
                      attempt.resume='BRIDGE_EXITED';
                    } else {
                      try {attempt.resume_ack=await control('resume_scanner');}
                      catch(e) {attempt.resume_error=e.message;throw e;}
                    }
                    save();
                   }
                 } else if(origin==='upstream' && (prior!==direction || observations.length%10===0)) {
                   save();
                 }
               }
               if(origin==='oc') {
                 const pending=stages.find(s=>!checks.stages[s]);
                 // Diagnostics from a few inexact pauses must not close the
                 // next native stage while an exact reference can still occur
                 // within the existing deadline.
                 if(pending && !referenceSignatures[pending] && candidates[pending]?.length &&
                     (candidates[pending].length>=3 || Date.now()-candidateWindows[pending]>=2000)) {
                  const best=candidates[pending].reduce((a,b)=>b.color_distance<a.color_distance?b:a);
                  checks.stages[pending]={...best,phase_match:'UNMATCHED_PHASE',
                    candidates_observed:candidates[pending].length};
                  save();
                }
              }
              if(bridgeExit!==undefined) throw Error('Bridge exited while scanner running: '+bridgeExit);
             await sleep(12);
           }
               for(const stage of stages) {
                 if(origin==='oc' && !checks.stages[stage] && candidates[stage]?.length) {
                   const best=candidates[stage].reduce((a,b)=>b.color_distance<a.color_distance?b:a);
                   checks.stages[stage]={...best,phase_match:'UNMATCHED_PHASE',
                     candidates_observed:candidates[stage].length};
                 }
                 if(!checks.stages[stage]) checks.stages[stage]={status:'UNMATCHED_PHASE',
                   reason:origin==='oc' ? referenceStages?.[stage]?.indicator_cells ?
                     'No paused native frame with original glyph pattern' : 'No original paused reference frame' :
                     'No paused stage frame'};
              }
               checks.predicates={all_phases_observed:stages.every(stage=>checks.stages[stage]?.status==='CAPTURED_SCANNER' &&
                 (origin==='upstream' || checks.stages[stage].phase_match==='EXACT_INDICATOR')),
              ...(scannerAnimation ? {forward_progression:forwardPositions.size>=1,
                end_fade_observed:seenEndFade,
                reverse_progression:reversePositions.size>=1,
                start_fade_observed:seenStartFade} : {}),
             provider_held:logs.some(e=>e.kind==='scanner_held'),
              no_early_completion:!logs.some(e=>e.kind==='scanner_resumed'),
              valid_running_requests:counts().invalid===0};
             save();
             if(scannerCancel) {
               const held=()=>logs.some(e=>e.kind==='scanner_held') && !logs.some(e=>
                 e.kind==='scanner_resumed' || e.kind==='provider_completed' && e.operation==='transcript');
               checks.interruption={escapes:[],armed_hint:null,status:'IN_PROGRESS'};
               const armed=observe(await runningProbe());
               checks.interruption.armed_hint=armed;
               checks.interruption.armed_counts=counts();save();
               if(!held() || armed.hints.length!==1 ||
                   (scannerAnimation ? armed.run_count!==1 : armed.fallback.length!==1))
                 throw Error('Scanner interrupt not armed with painted hint and held provider');
               let interrupted;
               const escapeDeadline=Date.now()+5000;
               for(let i=1;i<=3 && !interrupted && Date.now()<escapeDeadline;i++) {
                 const before=observe(await runningProbe());
                 if(!held() || before.hints.length!==1 ||
                     (scannerAnimation ? before.run_count!==1 : before.fallback.length!==1))
                   throw Error('Scanner no longer armed before Escape '+i);
                 const escape={number:i,at:new Date().toISOString(),held_before:held(),
                   before,counts_before:counts(),checks:[]};
                 checks.interruption.escapes.push(escape);
                 send('\x1b','scanner_interrupt_escape_'+i);save();
                 const checkUntil=Math.min(escapeDeadline,Date.now()+1100);
                 let absent=0;
                 while(Date.now()<checkUntil) {
                   const f=await frame(), observed=observe(f);
                   const gone=indicatorAbsent(f);
                   escape.checks.push({at:new Date().toISOString(),hint:observed.hints,
                     hint_text:observed.hint_text ?? null,
                     indicator_absent:gone,provider_counts:counts(),held:held()});
                   if(gone && ++absent>=2) {interrupted=f;break;}
                   if(!gone) absent=0;
                   if(bridgeExit!==undefined || !held()) break;
                   await sleep(100);
                 }
                 escape.counts_after=counts();escape.canceled=Boolean(interrupted);save();
               }
               if(!interrupted) {
                 checks.interruption.status='NO_CANCEL_AFTER_ESCAPES';
                 checks.interruption.provider_counts=counts();save();
                 throw Error('Scanner did not cancel after '+checks.interruption.escapes.length+' Escapes within five seconds');
               }
               // The idle tab title can keep animating independently of the
               // canceled footer. Freeze the real PTY before the full-grid PNG.
               let interruptedStatus;
               try {
                 checks.interruption.pause_ack=await control('pause_scanner');
                 interrupted=await frame();
                 if(!indicatorAbsent(interrupted) || !held())
                   throw Error('Scanner interruption changed before paused capture');
                 interruptedStatus=await capture('scanner-interrupted',interrupted,'CAPTURED_SCANNER_INTERRUPTED');
               } finally {
                 if(!child.stdin.destroyed && bridgeExit===undefined)
                   checks.interruption.resume_ack=await control('resume_scanner');
                 save();
               }
               checks.predicates={...checks.predicates,held_at_escape:checks.interruption.escapes.every(e=>e.held_before),
                 running_indicator_disappeared:indicatorAbsent(interrupted),
                 durable_input_visible:interrupted.text.includes(promptText),
                 interrupted_capture:interruptedStatus==='CAPTURED_SCANNER_INTERRUPTED',
                 no_completed_read:!interrupted.text.includes('GEOMETRY-SHORT: tool read completed.'),
                 no_completed_transcript:!logs.some(e=>e.kind==='provider_completed' && e.operation==='transcript')};
               Object.assign(checks.interruption,{status:'CANCELED',at:new Date().toISOString(),
                 provider_counts:counts(),grid_sha256:sha(JSON.stringify(interrupted)),indicator:observe(interrupted)});
               save();
             }
         } finally {
            // The server has a 60s timeout as a second bound; release even if
           // phase detection fails, so no handler holds bridge shutdown open.
            if(!child.stdin.destroyed) child.stdin.write(JSON.stringify({kind:'release_scanner'})+'\n');
            checks.release={at:new Date().toISOString(),elapsed_ms:Date.now()-started};save();
          }
          if(scannerCancel) {
             const after=await waitFor(f=>logs.some(e=>e.kind==='scanner_resumed') &&
               logs.some(e=>(e.kind==='provider_disconnected' || e.kind==='provider_completed') &&
                 e.operation==='transcript') &&
               !f.text.includes('esc interrupt'),'scanner canceled provider released',12000);
             const prior=observations.find(s=>s.hint)?.hint;
            checks.predicates={...checks.predicates,
              release_acknowledged:logs.some(e=>e.kind==='scanner_release_requested'),
              server_released:logs.some(e=>e.kind==='scanner_resumed' && e.released),
              provider_request_terminated:logs.some(e=>e.kind==='provider_disconnected' && e.operation==='transcript') ||
                logs.some(e=>e.kind==='provider_completed' && e.operation==='transcript'),
              no_post_cancel_answer:!after.text.includes('GEOMETRY-SHORT: tool read completed.'),
              durable_input_after_release:after.text.includes(promptText),
              no_hidden_spinner:!!prior && indicatorAbsent(after),
              no_extra_transcript_requests:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length===1 &&
                counts().invalid===0};
            checks.interruption.after_release_counts=counts();save();
          } else {
          const completed=await waitFor(f=>f.text.includes('GEOMETRY-SHORT: tool read completed.') &&
           logs.some(e=>e.kind==='scanner_resumed' && e.released) &&
           logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length===2 &&
           logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length===1 &&
           /MiMo-V2.6-Flash Free · \d/.test(f.text),'scanner released completed read',12000);
         const completedStatus=await capture('scanner-completed',completed,'CAPTURED_SCANNER_COMPLETED');
         const gone=observe(completed);
         const previous=observations.find(s=>s.hint)?.hint;
         const footer=previous ? completed.cells[previous.y] : [];
         const indicatorGone=gone.hints.length===0 && !footer.some(c=>c.symbol==='■' || c.symbol==='⬝') &&
           visibleMatches(completed,'[⋯]').filter(p=>p.y===previous?.y).length===0;
         checks.predicates={...checks.predicates,release_acknowledged:logs.some(e=>e.kind==='scanner_release_requested'),
           server_released:logs.some(e=>e.kind==='scanner_resumed' && e.released),
           running_indicator_disappeared:indicatorGone,
           completed_read_visible:completed.text.includes('GEOMETRY-SHORT: tool read completed.'),
           completed_capture:completedStatus==='CAPTURED_SCANNER_COMPLETED',
            valid_provider_roundtrip:counts().invalid===0 && counts().requests===3 && counts().completed===3};
          save();
          }
         const passed=Object.values(checks.predicates).every(Boolean);
          lock.scanner[origin].status=passed?'PASS':
            stages.some(stage=>checks.stages[stage]?.status==='UNMATCHED_PHASE' ||
              checks.stages[stage]?.phase_match==='UNMATCHED_PHASE') ? 'UNMATCHED_PHASE':'FAILED_OBSERVATION';
         lock.attempts.push({origin,status:'SCANNER_'+lock.scanner[origin].status,predicates:checks.predicates});
         if(!passed) result=1;
       } else {
       send('\x1b[200~'+fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim()+'\x1b[201~','prompt_paste');
      await sleep(200); send('\r','submit');
       const marker = ['short','reasoning','reasoning-steps','tools'].includes(args.sample) ? 'GEOMETRY-SHORT' : ['rows','rows-reflow'].includes(args.sample) ? 'ROW-089' : 'Через Code Mode';
        let done = await waitFor(f => f.text.includes(marker) &&
         (Number(args.columns) < 64 ? f.text.includes('Reader · MiMo-V2.6-Flash Free') :
           /MiMo-V2.6-Flash Free · \d/.test(f.text)) &&
         logs.some(e => e.kind==='provider_completed' && e.operation==='transcript') &&
          (!(reasoningClick || reasoningSteps || selectionCopy || toastOverlap || modelsInteraction) || logs.some(e=>e.kind==='provider_completed' && e.operation==='title')), 'completed transcript');
       if(twoTurn) done=await waitFor(f=>f.text.includes('GEOMETRY-SHORT: tool read completed.') &&
          /Reader · MiMo-V2.6-Flash Free · \d/.test(f.text) &&
          logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length===2 &&
          logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length===1,
          'first same-session read, answer and title');
       if(done.text.includes('opaque-fixture-must-not-display')) throw Error('opaque reasoning leaked to the terminal');
        const completedStatus = await capture('session-wide-completed',done,'CAPTURED');
        if(modelsInteraction) {
          const names=['Big Pickle','Ling 3.0 Flash Free','MiMo V2.5 Free','MiMo-V2.6-Flash Free',
            'Muse Spark 1.2 Free','Muse Spark 1.3 Free','Nemotron 3 Ultra Free',
            'Nemotron 3.5 Lightning Free',...Array.from({length:12},(_,i)=>`ZZ Scroll ${String(i).padStart(2,'0')}`)];
          const chosen='ZZ Scroll 11', chosenID='fixture-scroll-11';
          const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
            completed_transcript:logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length,
            title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
            completed_title:logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
          const baseline=counts();
          const unchanged=()=>canonical(counts())===canonical(baseline);
          const row=(f,name)=>{
            const title=visibleMatches(f,'Select model')[0];
            return title ? visibleMatches(f,name).filter(p=>p.y>title.y+3 && p.y<f.rows-5) : [];
          };
          const painted=(f)=>names.flatMap(name=>row(f,name).map(p=>({name,...p,
            fg:f.cells[p.y][p.x].fg,bg:f.cells[p.y][p.x].bg})));
          const checks=[];
          lock.models_interaction ??= {};
          lock.models_interaction[origin]={status:'IN_PROGRESS',baseline,chosen_id:chosenID,checks};
          const save=()=>{
            fs.writeFileSync(path.join(dir,'models-interaction-checks.json'),JSON.stringify({baseline,checks},null,2)+'\n');
            json('capture.lock.json',lock);
          };
          const check=(stage,f,predicates,details={})=>{
            checks.push({stage,painted_models:painted(f),current_marker:row(f,'●'),
              query:visibleMatches(f,chosen),provider_counts:counts(),predicates,...details,
              grid_sha256:sha(JSON.stringify(f))});
            save();
            if(Object.values(predicates).some(v=>v!==true)) throw Error('Models interaction predicate failed: '+stage);
          };
          const shot=async(stage,f)=>{
            const status=await capture('models-'+stage,f,'CAPTURED_MODELS_INTERACTION');
            check(stage+'-capture',await frame(),{stable_capture:status==='CAPTURED_MODELS_INTERACTION',
              no_request_before_next_turn:unchanged()});
          };
          check('first-turn',done,{stable_first_capture:completedStatus==='CAPTURED',
            completed_read_and_title:baseline.transcript===2 && baseline.completed_transcript===2 &&
              baseline.title===1 && baseline.completed_title===1 && baseline.invalid===0,
            first_answer_visible:done.text.includes('GEOMETRY-SHORT: tool read completed.')});
          send('\x18','models_open_ctrl_x'); await sleep(100);send('m','models_open_m');
          const opened=await waitFor(f=>f.text.includes('Select model') && row(f,names[0]).length===1 &&
            row(f,'●').length===1 && unchanged(),'Models initial grid',12000);
          const initial=painted(opened);
          const initialFocus=initial.find(p=>p.name===names[0]);
          const expectedVisible=Math.min(names.length,Math.floor(opened.rows/2)-6);
          check('initial',opened,{expected_models_painted:initial.length===expectedVisible &&
              initial.every((p,i)=>p.name===names[i]),
            current_model_marked:row(opened,'●')[0]?.y===row(opened,'MiMo-V2.6-Flash Free')[0]?.y,
            first_option_visible:!!initialFocus,focused_not_current:initialFocus?.y!==row(opened,'●')[0]?.y,
            distinct_focus_background:initialFocus!==undefined && initial.some(p=>p.bg!==initialFocus.bg),
            no_provider_request:unchanged()}, {visible_model_count:initial.length,
              integration_action:visibleMatches(opened,'View all integrations'),
              integration_action_visible:opened.text.includes('View all integrations')});
          await shot('initial',opened);
          // Pinned DialogModel focusCurrent=false. Walk 16 real Down events,
          // across the eight original fixture entries and beyond the viewport.
          let previous=opened;
          for(let i=1;i<=16;i++) {
            send('\x1b[B',`models_down_${i}`);
            const before=sha(JSON.stringify(previous));
            previous=await waitFor(f=>f.text.includes('Select model') &&
              sha(JSON.stringify(f))!==before && unchanged(),`Models Down ${i}`,12000);
          }
          const scrolled=previous;
          const scrolledMarker=row(scrolled,'●');
          const scrolledCurrent=row(scrolled,'MiMo-V2.6-Flash Free');
          check('scrolled',scrolled,{new_model_visible:row(scrolled,'ZZ Scroll 08').length===1,
            first_model_scrolled_off:row(scrolled,names[0]).length===0,
            new_model_focused:painted(scrolled).find(p=>p.name==='ZZ Scroll 08')?.bg===initialFocus.bg,
            visible_marker_on_current_model:scrolledMarker.length===0 || (scrolledMarker.length===1 &&
              scrolledCurrent.some(p=>p.y===scrolledMarker[0].y)),
            no_provider_request:unchanged()}, {observations:{offscreen_current_marker_absent:scrolledMarker.length===0,
              current_model_visible:scrolledCurrent.length===1,
              current_marker_on_current_model:scrolledMarker.length===1 &&
                scrolledCurrent.some(p=>p.y===scrolledMarker[0].y)}});
          await shot('scrolled',scrolled);
          send(chosen,'models_filter_query');
          const filtered=await waitFor(f=>f.text.includes('Select model') &&
            row(f,chosen).length===1 && visibleMatches(f,chosen).length>=2 && unchanged(),
            'Models filtered query',12000);
          const targetRows=visibleMatches(filtered,chosen);
          const targetOption=painted(filtered).filter(p=>p.name===chosen);
          check('filtered',filtered,{query_and_option_painted:targetRows.length>=2 && targetOption.length===1,
            filtered_target_focused:targetOption[0]?.bg===initialFocus.bg,
            target_distinct_from_current:!row(filtered,'●').some(p=>p.y===targetOption[0]?.y),
            no_provider_request:unchanged()}, {target_rows:targetRows,
              current_marker_visible:row(filtered,'●').length>0});
          await shot('filtered',filtered);
          send('\r','models_select_filtered_return');
          const selected=await waitFor(f=>!f.text.includes('Select model') &&
            f.text.includes('Reader · ZZ Scroll 11') &&
            f.text.includes('GEOMETRY-SHORT: tool read completed.') && unchanged(),
            'selected model in same session',12000);
          check('selected',selected,{dialog_closed:!selected.text.includes('Select model'),
            new_model_in_composer:selected.text.includes('Reader · ZZ Scroll 11'),
            old_answer_preserved:selected.text.includes('GEOMETRY-SHORT: tool read completed.'),
            no_provider_request:unchanged()});
          await shot('selected',selected);
          const secondPrompt='Second same-session model check?';
          send('\x1b[200~'+secondPrompt+'\x1b[201~','models_second_prompt_paste');
          await waitFor(f=>f.text.includes(secondPrompt) && unchanged(),'Models second draft',12000);
          send('\r','models_second_turn_submit');
          const finished=await waitFor(f=>f.text.includes('GEOMETRY-TURN-TWO: tool read completed.') &&
            f.text.includes('GEOMETRY-SHORT: tool read completed.') &&
            /Reader · ZZ Scroll 11 · \d/.test(f.text) && counts().transcript===4 &&
            counts().completed_transcript===4 && counts().title===1 &&
            counts().completed_title===1 && counts().invalid===0,'Models second real read',20000);
          const requests=logs.filter(e=>e.kind==='provider');
          const after=counts();
          const predicates={same_session_title:visibleMatches(finished,oldTitlePrefix).some(p=>p.y===0),
            first_turn_original_model:requests.filter(e=>e.operation==='transcript' && e.turn_number===0).length===2 &&
              requests.filter(e=>e.operation==='transcript' && e.turn_number===0).every(e=>e.model==='fixture-model-1' && e.valid),
            second_turn_selected_model:requests.filter(e=>e.operation==='transcript' && e.turn_number===1).length===2 &&
              requests.filter(e=>e.operation==='transcript' && e.turn_number===1).every(e=>e.model===chosenID && e.valid),
            second_read_result:requests.some(e=>e.turn_number===1 && e.round_number===1 && e.fixture_content_returned),
            no_named_variant:requests.filter(e=>e.turn_number===1 && e.operation==='transcript')
              .every(e=>!e.request_top_level_keys.includes('variant')),
            complete_counts:after.transcript===4 && after.completed_transcript===4 &&
              after.title===1 && after.completed_title===1 && after.invalid===0};
          const finalStatus=await capture('models-second-turn',finished,'CAPTURED_MODELS_SECOND_TURN');
          checks.push({stage:'second-turn',predicates:{...predicates,stable_capture:finalStatus==='CAPTURED_MODELS_SECOND_TURN'},
            requests:requests.map(e=>({model:e.model,operation:e.operation,turn_number:e.turn_number,
              round_number:e.round_number,valid:e.valid,request_sha256:e.request_sha256,
              request_top_level_keys:e.request_top_level_keys})),provider_counts:after});
          save();
          if(Object.values(checks.at(-1).predicates).some(v=>v!==true)) throw Error('Models second-turn provider predicate failed');
          lock.models_interaction[origin].status='PASS';save();
          lock.attempts.push({origin,status:'MODELS_INTERACTION_PASS',provider_counts:after,
            selected_model:chosenID,predicates:checks.at(-1).predicates});
        }
       if(twoTurn) {
          const checks=[];
          lock.two_turn ??= {};
          lock.two_turn[origin]={status:'IN_PROGRESS',checks};
          const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
            completed_transcript:logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length,
            title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
            completed_title:logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
          const save=()=>{
            fs.writeFileSync(path.join(dir,'two-turn-checks.json'),JSON.stringify({checks,provider_counts:counts()},null,2)+'\n');
            json('capture.lock.json',lock);
          };
          const firstPrompt=fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim();
          const secondPrompt='Second same-session spacing check?';
          const firstAnswer='GEOMETRY-SHORT: tool read completed.';
          const secondAnswer='GEOMETRY-TURN-TWO: tool read completed.';
          const unique=(f,needle)=>{
            const found=visibleMatches(f,needle);
            return found.length===1 ? found[0] : null;
          };
          const footer=(f,after,before=f.rows)=>f.cells.map((row,y)=>({y,text:row.map(c=>c.symbol).join('')}))
            .filter(row=>row.y>after && row.y<before && /Reader · MiMo-V2\.6-Flash Free · \d/.test(row.text));
          const baseline=counts();
          checks.push({stage:'first-completed',provider_counts:baseline,
            predicates:{first_capture_stable:completedStatus==='CAPTURED',
              one_read_answer_title:baseline.transcript===2 && baseline.completed_transcript===2 &&
                baseline.title===1 && baseline.completed_title===1 && baseline.invalid===0,
              first_prompt_unique:!!unique(done,firstPrompt),first_answer_unique:!!unique(done,firstAnswer),
              first_footer_unique:footer(done,unique(done,firstAnswer)?.y ?? done.rows).length===1}});
          save();
          if(Object.values(checks.at(-1).predicates).some(v=>!v)) throw Error('First same-session turn not complete');
          send('\x1b[200~'+secondPrompt+'\x1b[201~','two_turn_prompt_paste');
          const drafted=await waitFor(f=>!!unique(f,secondPrompt) && counts().transcript===2,
            'second draft in same session',12000);
          checks.push({stage:'second-draft',prompt_position:unique(drafted,secondPrompt),provider_counts:counts()});
          save();
          send('\r','two_turn_submit');
          const complete=await waitFor(f=>!!unique(f,firstPrompt) && !!unique(f,secondPrompt) &&
            !!unique(f,firstAnswer) && !!unique(f,secondAnswer) &&
            footer(f,unique(f,firstAnswer).y,unique(f,secondPrompt).y).length===1 &&
            footer(f,unique(f,secondAnswer).y).length===1 &&
            counts().transcript===4 && counts().completed_transcript===4 &&
            counts().title===1 && counts().completed_title===1 && counts().invalid===0,
            'two completed turns in same session',20000);
          const first=unique(complete,firstAnswer), user=unique(complete,secondPrompt), second=unique(complete,secondAnswer);
          const firstFooter=footer(complete,first.y,user.y)[0];
          const secondFooter=footer(complete,second.y)[0];
          const blockRows=complete.cells.map((cells,y)=>({y,
            x:cells.findIndex((c,x)=>x<user.x && c.symbol==='┃')}))
            .filter(p=>p.y>firstFooter.y && p.y<=user.y && p.x>=0);
          const blockTop=blockRows.length?blockRows[0].y:null;
          const betweenRows=complete.cells.slice(firstFooter.y+1,user.y).map((cells,index)=>({
            y:firstFooter.y+1+index, text:cells.map(c=>c.symbol).join(''), cells}));
          const positions={first_answer:first,first_footer:{y:firstFooter.y,text:firstFooter.text},
            second_user:user,second_user_block_top:blockTop,second_user_block_cells:blockRows,
            second_footer:{y:secondFooter.y,text:secondFooter.text},
            answer_to_footer_rows:firstFooter.y-first.y-1,
            footer_to_user_block_rows:blockTop===null?null:blockTop-firstFooter.y-1,
            footer_to_user_text_rows:user.y-firstFooter.y-1,between_rows:betweenRows};
          const status=await capture('session-two-turn-completed',complete,'CAPTURED_TWO_TURN');
          const after=counts();
          const predicates={stable_capture:status==='CAPTURED_TWO_TURN',
            same_session_title:visibleMatches(complete,[...tabTitle].slice(0,6).join('')).some(p=>p.y===0),
            ordered_rows:first.y<firstFooter.y && firstFooter.y<blockTop && blockTop<user.y &&
              user.y<second.y && second.y<secondFooter.y,
            two_valid_read_roundtrips:after.transcript===4 && after.completed_transcript===4 &&
              after.title===1 && after.completed_title===1 && after.invalid===0 &&
              [0,1].every(turn=>[0,1].every(round=>logs.some(e=>e.kind==='provider' &&
                e.operation==='transcript' && e.turn_number===turn && e.round_number===round && e.valid)))};
          checks.push({stage:'second-completed',positions,provider_counts:after,predicates});
          save();
          if(Object.values(predicates).some(v=>!v)) throw Error('Same-session second-turn predicate failed');
          lock.two_turn[origin].status='PASS';
          save();
          lock.attempts.push({origin,status:'TWO_TURN_CHECKS_PASS',provider_counts:after,
            answer_to_footer_rows:positions.answer_to_footer_rows,
            footer_to_user_block_rows:positions.footer_to_user_block_rows,
            footer_to_user_text_rows:positions.footer_to_user_text_rows});
        }
        if(toastOverlap) {
          const longTitle='OVERLAPTITLE-'.repeat(8);
          const checks=[];
          const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
            completed:logs.filter(e=>e.kind==='provider_completed').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
          const baseline=counts();
          const unchanged=()=>canonical(counts())===canonical(baseline);
          lock.toast_overlap ??= {};
          lock.toast_overlap[origin]={status:'IN_PROGRESS',baseline,checks,
            clipboard_destination:'NOT_VERIFIED_BY_PTY'};
          const save=()=>{
            fs.writeFileSync(path.join(dir,'toast-overlap-checks.json'),JSON.stringify({baseline,checks},null,2)+'\n');
            json('capture.lock.json',lock);
          };
          const record=(stage,predicates,details={})=>{
            checks.push({stage,predicates,provider_counts:counts(),...details});save();
            if(Object.values(predicates).some(v=>v!==true)) throw Error('Toast overlap predicate failed: '+stage);
          };
          const word='GEOMETRY';
          record('completed-read',{
            stable_capture:completedStatus==='CAPTURED',unique_answer_word:visibleMatches(done,word).length===1,
            sidebar_visible:visibleMatches(done,'Context').some(p=>p.x>=80 && p.y<20),
            read_and_title_complete:baseline.requests===3 && baseline.completed===3 && baseline.invalid===0});
          send('\x12','toast_overlap_rename_ctrl_r');
          const prefilled=await waitFor(f=>f.text.includes('Rename session') &&
            visibleMatches(f,tabTitle).some(p=>p.y>0) && unchanged(),'toast overlap rename prefill');
          record('rename-prefilled',{dialog_visible:prefilled.text.includes('Rename session'),provider_unchanged:unchanged()});
          send('\x1b[H','toast_overlap_rename_home');await sleep(100);
          send('\x1b[1;2F','toast_overlap_rename_shift_end');await sleep(100);
          send(longTitle,'toast_overlap_rename_replacement');
          const edited=await waitFor(f=>visibleMatches(f,'OVERLAPTITLE-').some(p=>p.y>0) &&
            !visibleMatches(f,tabTitle).some(p=>p.y>0) && unchanged(),'toast overlap rename edited');
          record('rename-edited',{replacement_visible:visibleMatches(edited,'OVERLAPTITLE-').some(p=>p.y>0),
            provider_unchanged:unchanged()});
          send('\r','toast_overlap_rename_return');
          // The long title is persisted by the real session.rename route. Require
          // visible painted underlay in the toast's lower padding row (y=3).
          const underlay=await waitFor(f=>!f.text.includes('Rename session') &&
            visibleMatches(f,word).length===1 && f.cells[3].slice(90).some(c=>/[A-Z]/.test(c.symbol)) &&
            visibleMatches(f,'Context').some(p=>p.x>=80 && p.y<20) && unchanged(),
            'renamed sidebar title under toast padding');
          const underlayRow=underlay.cells[3].slice(90).map(c=>c.symbol).join('');
          record('title-underlay',{
            title_in_sidebar:underlay.cells.slice(2,6).some(row=>row.slice(80).map(c=>c.symbol).join('').includes('OVERLAP')),
            title_on_toast_padding_row:/[A-Z]/.test(underlayRow),
            unique_answer_word:visibleMatches(underlay,word).length===1,
            no_toast:visibleMatches(underlay,'Copied to clipboard').length===0,
            provider_unchanged:unchanged()}, {underlay_row_y3_from_x90:underlayRow,long_title:longTitle});
          const beforeStatus=await capture('toast-overlap-before',underlay,'CAPTURED_TOAST_UNDERLAY');
          record('before-capture',{stable_capture:beforeStatus==='CAPTURED_TOAST_UNDERLAY',provider_unchanged:unchanged()});
          const target=visibleMatches(underlay,word)[0];
          const x=target.x+4,y=target.y+1; // interior of painted transcript word, one-based SGR
          const osc52=()=> (Buffer.concat(chunks[generation]).toString('latin1').match(/\x1b\]52;/g)||[]).length;
          const beforeOsc52=osc52();
          for(let i=0;i<2;i++) {
            const down=`\x1b[<0;${x};${y}M`,up=`\x1b[<0;${x};${y}m`;
            checks.push({stage:'mouse-click-'+(i+1),cell:{x:x-1,y:y-1},pty_column:x,pty_row:y,
              down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});save();
            send(down,`toast_overlap_mouse_down_${i+1}`);send(up,`toast_overlap_mouse_up_${i+1}`);
            await sleep(80);
          }
          const toast=await waitFor(f=>visibleMatches(f,'Copied to clipboard').length===1 &&
            visibleMatches(f,word).length===1 && unchanged(),'copied toast over long sidebar title',3200);
          const message=visibleMatches(toast,'Copied to clipboard')[0];
          const left=message.x-3, right=toast.cells[message.y].findIndex((c,index)=>index>message.x+20 && c.symbol==='┃');
          const paddingY=message.y+1;
          const underlayCells=underlay.cells[paddingY]?.slice(left+1,right) ?? [];
          const paddingCells=toast.cells[paddingY]?.slice(left+1,right) ?? [];
          const toastBg=toast.cells[message.y]?.[message.x]?.bg;
          const status=await capture('toast-overlap-toast',toast,'CAPTURED_TOAST_OVERLAP');
          record('toast-capture',{
            stable_capture:status==='CAPTURED_TOAST_OVERLAP',unique_toast_message:visibleMatches(toast,'Copied to clipboard').length===1,
            toast_side_borders:left>=0 && right>left && [message.y-1,message.y,paddingY].every(row=>
              toast.cells[row]?.[left]?.symbol==='┃' && toast.cells[row]?.[right]?.symbol==='┃'),
            title_overlaps_padding:underlayCells.some(c=>/[A-Z]/.test(c.symbol)),
            blank_padding:paddingCells.length>0 && paddingCells.every(c=>c.symbol===' ' && c.bg===toastBg),
            selected_word_styled:word.split('').some((_,i)=>
              canonical(toast.cells[target.y][target.x+i])!==canonical(underlay.cells[target.y][target.x+i])),
            osc52_attempted:osc52()>beforeOsc52,provider_unchanged:unchanged()
          },{toast_rect:{x:left,y:message.y-1,width:right-left+1,height:3},
            underlying_padding:underlayCells,painted_padding:paddingCells,
            osc52_introducers_since_mouse:osc52()-beforeOsc52,
            clipboard_destination:'NOT_VERIFIED_BY_PTY; OSC 52 prefix and toast do not prove a system clipboard write'});
          lock.toast_overlap[origin].status='PASS';save();
          lock.attempts.push({origin,status:'TOAST_OVERLAP_CHECKS_PASS',provider_counts:counts(),
            clipboard_destination:'NOT_VERIFIED_BY_PTY'});
        }
       if(selectionCopy) {
         const word='GEOMETRY';
         const checks=[];
         const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
           completed:logs.filter(e=>e.kind==='provider_completed').length,
           invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
         const baseline=counts();
         const unchanged=()=>canonical(counts())===canonical(baseline);
         lock.selection_copy ??= {};
         lock.selection_copy[origin]={status:'IN_PROGRESS',clipboard_mode:'default Select',baseline,checks};
         const save=()=>{
           fs.writeFileSync(path.join(dir,'selection-copy-checks.json'),JSON.stringify({baseline,checks},null,2)+'\n');
           json('capture.lock.json',lock);
         };
         const toast=f=>visibleMatches(f,'Copied to clipboard');
         const matches=visibleMatches(done,word);
         if(matches.length!==1 || completedStatus!=='CAPTURED' || baseline.requests!==3 ||
             baseline.completed!==3 || baseline.invalid!==0)
           throw Error('Selection copy requires one painted answer word, stable completed read/title and valid provider baseline');
         const target=matches[0];
         const cell={x:target.x+3,y:target.y};
         const selectionStyle=c=>({fg:c.fg,bg:c.bg,modifiers:c.modifiers});
         const before=await frame();
         if(canonical(visibleMatches(before,word))!==canonical([target]) || toast(before).length || !unchanged())
           throw Error('Selection copy baseline changed before mouse input');
         const baseRow=before.cells[target.y];
         const observe=f=>{
           const painted=visibleMatches(f,word);
           const row=f.cells[target.y];
           const changed=row.map((c,x)=>canonical(selectionStyle(c))!==canonical(selectionStyle(baseRow[x])) ? x : -1).filter(x=>x>=0);
           return {word_matches:painted,toast_matches:toast(f),changed_style_columns:changed,
             word_highlight_columns:changed.filter(x=>x>=target.x && x<target.x+word.length),
             outside_word_highlight_columns:changed.filter(x=>x<target.x || x>=target.x+word.length),
             word_cells:row.slice(target.x,target.x+word.length).map(selectionStyle),
             provider_counts:counts()};
         };
         checks.push({stage:'completed-before-click',target,cell,word_cells:baseRow.slice(target.x,target.x+word.length).map(selectionStyle),
           predicates:{unique_painted_word:matches.length===1,completed_capture:completedStatus==='CAPTURED',
             default_select_mode:true,no_prior_toast:toast(before).length===0,provider_counts_unchanged:unchanged()}});
         save();
         // Two and three independent press/release pairs at the same cell. No
         // motion is synthesized; the original may require OpenTUI isDragging.
         const pair=async(stage,index)=>{
           const x=cell.x+1,y=cell.y+1;
           const down=`\x1b[<0;${x};${y}M`,up=`\x1b[<0;${x};${y}m`;
           checks.push({stage:`${stage}-click-${index}`,cell,pty_column:x,pty_row:y,
             down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});
           save();
           send(down,`selection_${stage}_${index}_mouse_down`);
           send(up,`selection_${stage}_${index}_mouse_up`);
           await sleep(80);
         };
         let passed=true;
         const osc52Count=()=> (Buffer.concat(chunks[generation]).toString('latin1').match(/\x1b\]52;/g)||[]).length;
         let previousOsc52=osc52Count();
         for(const [stage,repetitions] of [['double',2],['triple',3]]) {
           if(stage==='triple') {
             // A prior toast can overlay a new toast and expire during the PNG.
             // Wait for observed disappearance, also resetting the multi-click
             // interval rather than assuming a fixed toast lifetime.
             try {
               const cleared=await waitFor(f=>toast(f).length===0 && visibleMatches(f,word).length===1 && unchanged(),
                 'selection copy prior toast disappeared',12000);
               checks.push({stage:'between-clicks-toast-expired',observed:observe(cleared),predicates:{toast_absent:true,provider_counts_unchanged:unchanged()}});
             } catch(e) {
               checks.push({stage:'between-clicks-toast-expired',observed:observe(await frame()),predicates:{toast_absent:false},reason:e.message});
               passed=false;
             }
             save();
             await sleep(600);
           }
           for(let index=1;index<=repetitions;index++) await pair(stage,index);
           // The toast can arrive asynchronously and is short-lived. Observe
           // both styled selection and real painted feedback before screenshot.
           let observed;
           try {observed=await waitFor(f=>toast(f).length===1 && observe(f).word_highlight_columns.length>0 && unchanged(),
             `selection ${stage} highlight and copy toast`,3200);}
           catch(e) {
             observed=await frame();
             checks.push({stage:`${stage}-wait`,observed:observe(observed),reason:e.message});
             save();
           }
           const snapshot=observe(observed);
           const predicates={unique_painted_word:canonical(snapshot.word_matches)===canonical([target]),
             word_highlight_visible:snapshot.word_highlight_columns.length>0,
             toast_visible:snapshot.toast_matches.length===1,
             line_extends_beyond_word:stage==='triple' ? snapshot.outside_word_highlight_columns.length>0 : null,
             provider_counts_unchanged:unchanged()};
           const status=await capture(`selection-${stage}`,observed,'CAPTURED_SELECTION_COPY');
           predicates.stable_capture=status==='CAPTURED_SELECTION_COPY';
           // Record only the OSC 52 introducer count; never decode or store a
           // clipboard payload in checks/lock. Raw VT is existing capture data.
           const osc52=osc52Count();
           checks.push({stage:`${stage}-captured`,observed:snapshot,predicates,
             osc52_introducers_since_previous_stage:osc52-previousOsc52,
             osc52_note:'Prefix-only observation in PTY output; not proof of a system clipboard write'});
           previousOsc52=osc52;
           save();
           if(Object.entries(predicates).some(([,v])=>v===false)) passed=false;
         }
         lock.selection_copy[origin].status=passed?'PASS':'FAILED_OBSERVATION';
         save();
         lock.attempts.push({origin,status:passed?'SELECTION_COPY_CHECKS_PASS':'SELECTION_COPY_CHECKS_FAILED',
           predicates:checks.filter(c=>c.predicates).map(c=>({stage:c.stage,...c.predicates}))});
         if(!passed) result=1;
       }
       if(reasoningClick) {
          const checks=[];
          const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
            title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
            completed_transcript:logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length,
            completed_title:logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
          const baseline=counts();
          const providerStable=()=>canonical(counts())===canonical(baseline);
          // The pinned hide-mode group header (session/index.tsx) is a painted
          // InlineToolRow, not a string offset in the concatenated frame text.
          const observe=f=>{
            const collapsed=visibleMatches(f,'+ Thought: Inspecting');
            const expanded=visibleMatches(f,'- Thought');
            const fragments=visibleMatches(f,'Thought');
            const bodies=visibleMatches(f,'Public summary only.');
            const header=collapsed.length===1 && expanded.length===0 ? {...collapsed[0],state:'collapsed'} :
              expanded.length===1 && collapsed.length===0 ? {...expanded[0],state:'expanded'} : null;
            const text=header?.state==='collapsed' ? '+ Thought: Inspecting' : '- Thought';
            const cells=header ? f.cells[header.y].slice(header.x,header.x+[...text].length) : [];
            return {collapsed,expanded,fragments,bodies,header,
              header_style:header ? {fg:cells.map(c=>c.fg),bg:cells.map(c=>c.bg),
                modifiers:cells.map(c=>c.modifiers),widths:cells.map(c=>c.width)} : null,
              cursor:f.cursor,provider_counts:counts(),grid_sha256:sha(JSON.stringify(f))};
          };
          const save=()=>{
            fs.writeFileSync(path.join(dir,'reasoning-click-checks.json'),JSON.stringify({baseline,checks},null,2)+'\n');
            json('capture.lock.json',lock);
          };
          lock.reasoning_click ??= {};
          lock.reasoning_click[origin]={status:'IN_PROGRESS',baseline,checks};
          const record=(stage,f,predicates,extra={})=>{
            const observed=observe(f);
            checks.push({stage,observed,predicates,...extra});
            save();
            if(Object.values(predicates).some(value=>value!==true))
              throw Error('Reasoning click predicate failed: '+stage);
            return observed;
          };
          const unique=o=>o.fragments.length===1 && o.collapsed.length+o.expanded.length===1 &&
            o.header && o.fragments[0].x===o.header.x+2 && o.fragments[0].y===o.header.y;
          const styled=o=>o.header_style?.fg.every(fg=>fg!=='#eeeeee') &&
            o.header_style?.widths.every(width=>width===1) && o.header_style.fg.length>0;
          const bodyBelow=o=>o.bodies.length===1 && o.header && o.bodies[0].y>o.header.y;
          const cursorValid=f=>f.cursor.x>=0 && f.cursor.x<f.columns &&
            f.cursor.y>=0 && f.cursor.y<f.rows;
          const state=(f,kind)=>{
            const o=observe(f);
            if(o.fragments.length>1 || o.collapsed.length>1 || o.expanded.length>1 || o.bodies.length>1)
              throw Error('Duplicate/overpainted reasoning header or body: '+kind+' '+JSON.stringify({
                fragments:o.fragments,collapsed:o.collapsed,expanded:o.expanded,bodies:o.bodies}));
            return unique(o) && o.header.state===kind && styled(o) &&
              (kind==='collapsed' ? o.bodies.length===0 : bodyBelow(o)) &&
              providerStable() && cursorValid(f) && f.text.includes('GEOMETRY-SHORT: public reasoning completed.');
          };
          const awaitState=async(stage,kind)=>{
            try {return await waitFor(f=>state(f,kind),`reasoning click ${stage}`,12000);}
            catch(e) {
              const current=await frame();
              checks.push({stage:stage+'-failed',observed:observe(current),
                predicates:{state_reached:false},reason:e.message});
              save();
              throw e;
            }
          };
          const check=(stage,f,kind)=>{
            const o=observe(f);
            return record(stage,f,{
              unique_painted_header:!!unique(o),
              expected_header:o.header?.state===kind,
              styled_header:!!styled(o),
              body_visibility:kind==='collapsed' ? o.bodies.length===0 : !!bodyBelow(o),
              provider_counts_unchanged:providerStable(),cursor_in_bounds:cursorValid(f),
              completed_answer_visible:f.text.includes('GEOMETRY-SHORT: public reasoning completed.'),
              opaque_content_absent:!f.text.includes('opaque-fixture-must-not-display')});
          };
          const shot=async(stage,f)=>{
            const status=await capture(`reasoning-${stage}`,f,'CAPTURED_REASONING_CLICK');
            record(stage+'-capture',await frame(),{stable_capture:status==='CAPTURED_REASONING_CLICK'});
          };
          const click=(stage,f)=>{
            const o=observe(f);
            const x=o.header.x+3, y=o.header.y+1; // interior 'T', one-based SGR
            const down=`\x1b[<0;${x};${y}M`,up=`\x1b[<0;${x};${y}m`;
            record(stage+'-click',f,{unique_painted_header:!!unique(o),
              expected_header:o.header?.state===(stage==='expand'?'collapsed':'expanded'),
              body_visibility:stage==='expand'?o.bodies.length===0:!!bodyBelow(o),
              provider_counts_unchanged:providerStable(),cursor_in_bounds:cursorValid(f)},
              {cell:{x:o.header.x+2,y:o.header.y},pty_column:x,pty_row:y,
                input_mode:reasoningReleaseOnly?'release_only':'press_and_release',
                ...(!reasoningReleaseOnly ? {down_base64:Buffer.from(down).toString('base64')} : {}),
                up_base64:Buffer.from(up).toString('base64')});
            if (!reasoningReleaseOnly) send(down,`reasoning_${stage}_mouse_down`);
            send(up,`reasoning_${stage}_mouse_up`);
          };
          record('provider-baseline',done,{completed_session_captured:completedStatus==='CAPTURED',
            one_completed_transcript:baseline.transcript===1 && baseline.completed_transcript===1,
            one_completed_title:baseline.title===1 && baseline.completed_title===1,
            no_invalid_requests:baseline.invalid===0});
          const collapsed=await awaitState('collapsed','collapsed');
          check('collapsed',collapsed,'collapsed');
          await shot('collapsed',collapsed);
          click('expand',await awaitState('before-expand-click','collapsed'));
          const expanded=await awaitState('expanded','expanded');
          check('expanded',expanded,'expanded');
          await shot('expanded',expanded);
          click('recollapse',await awaitState('before-recollapse-click','expanded'));
          const recollapsed=await awaitState('recollapsed','collapsed');
          check('recollapsed',recollapsed,'collapsed');
          await shot('recollapsed',recollapsed);
          lock.reasoning_click[origin].status='PASS';
          save();
          lock.attempts.push({origin,status:'REASONING_CLICK_CHECKS_PASS',provider_counts:counts(),
            predicates:checks.filter(c=>c.predicates).map(c=>({stage:c.stage,...c.predicates}))});
        }
        if(reasoningSteps) {
          const checks=[];
          const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
            completed_transcript:logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length,
            title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
            completed_title:logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length,
            tool_results:logs.filter(e=>e.kind==='provider' && e.operation==='transcript' && e.tool_result_count>0).length});
          const baseline=counts();
          const unchanged=()=>canonical(counts())===canonical(baseline);
          lock.reasoning_steps ??= {};
          lock.reasoning_steps[origin]={status:'IN_PROGRESS',baseline,checks};
          const save=()=>{
            fs.writeFileSync(path.join(dir,'reasoning-steps-checks.json'),JSON.stringify({baseline,checks,provider_counts:counts()},null,2)+'\n');
            json('capture.lock.json',lock);
          };
          const markers=['First public step marker.','Second public step marker.'];
          const answer='GEOMETRY-SHORT: two public reasoning steps completed.';
          const observe=f=>{
            const headers=f.cells.flatMap((row,y)=>{
              const line=row.map(c=>c.symbol).join('');
              return [...line.matchAll(/([+-]) Thought(?:: ([^·]*?))?(?: · (\d+) steps)?(?= · (?!\d+ steps)| {2,}|$)/g)]
                .map(match=>({x:visibleMatches(f,match[0]).find(p=>p.y===y)?.x ?? null,
                  y,text:match[0].trimEnd(),state:match[1]==='+'?'collapsed':'expanded',
                  title:match[2]?.trim() ?? null,steps:match[3]===undefined?null:Number(match[3]),
                  cells:row.slice(match.index,match.index+[...match[0]].length)}));
            });
            return {headers,body_positions:markers.map(marker=>visibleMatches(f,marker)),
              title_positions:['Inspecting','Verifying'].map(title=>visibleMatches(f,title)),
              answer_positions:visibleMatches(f,answer),opaque_absent:!f.text.includes('opaque-fixture-must-not-display'),
              cursor:f.cursor,provider_counts:counts(),grid_sha256:sha(JSON.stringify(f))};
          };
          const record=(stage,f,predicates,details={})=>{
            checks.push({stage,observed:observe(f),predicates,...details});save();
            if(Object.values(predicates).some(value=>value!==true))
              throw Error('Reasoning steps predicate failed: '+stage);
          };
          const state=(f,kind)=>{
            const o=observe(f), h=o.headers[0];
            return o.headers.length===1 && h.x!==null && h.state===kind && h.steps===2 &&
              h.cells.every(c=>c.width===1) && o.answer_positions.length===1 && o.opaque_absent &&
              (kind==='collapsed' ? o.body_positions.every(p=>p.length===0) :
                o.body_positions.every(p=>p.length===1 && p[0].y>h.y) &&
                o.body_positions[0][0].y<o.body_positions[1][0].y) && unchanged();
          };
          const awaitState=async(stage,kind)=>{
            try {return await waitFor(f=>state(f,kind),`reasoning steps ${stage}`,12000);}
            catch(e) {
              checks.push({stage:stage+'-failed',observed:observe(await frame()),
                predicates:{state_reached:false},reason:e.message});save();throw e;
            }
          };
          const check=(stage,f,kind)=>{
            const o=observe(f), h=o.headers[0];
            record(stage,f,{one_group:o.headers.length===1,header_state:h?.state===kind,
              two_steps:h?.steps===2,header_title:kind==='collapsed'?h?.title==='Verifying':h?.title===null,
              distinct_bodies:kind==='collapsed'?o.body_positions.every(p=>p.length===0):
                o.body_positions.every(p=>p.length===1 && p[0].y>h?.y) &&
                  o.body_positions[0][0].y<o.body_positions[1][0].y,
              titles_visible:kind==='collapsed'?o.title_positions[0].length===0:
                o.title_positions.every(p=>p.length===1 && p[0].y>h?.y) &&
                  o.title_positions[0][0].y<o.title_positions[1][0].y,
              answer_visible:o.answer_positions.length===1,opaque_absent:o.opaque_absent,
              provider_unchanged:unchanged()});
          };
          const shot=async(stage,f)=>{
            const status=await capture(`reasoning-steps-${stage}`,f,'CAPTURED_REASONING_STEPS');
            record(stage+'-capture',await frame(),{stable_capture:status==='CAPTURED_REASONING_STEPS'});
          };
          const click=(stage,f)=>{
            const o=observe(f),h=o.headers[0];
            record(stage+'-before-click',f,{one_group:o.headers.length===1,
              expected_header:h?.state===(stage==='expand'?'collapsed':'expanded'),
              provider_unchanged:unchanged()});
            // One-based SGR coordinates for the painted 'T' of this side's sole header.
            const x=h.x+3,y=h.y+1,down=`\x1b[<0;${x};${y}M`,up=`\x1b[<0;${x};${y}m`;
            checks.push({stage:stage+'-input',cell:{x:x-1,y:y-1},pty_column:x,pty_row:y,
              down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});save();
            send(down,`reasoning_steps_${stage}_mouse_down`);
            send(up,`reasoning_steps_${stage}_mouse_up`);
          };
          record('provider-baseline',done,{stable_completed_capture:completedStatus==='CAPTURED',
            one_completed_transcript:baseline.transcript===1 && baseline.completed_transcript===1,
            one_completed_title:baseline.title===1 && baseline.completed_title===1,
            no_invalid:baseline.invalid===0,no_tool:baseline.tool_results===0,
            answer_visible:observe(done).answer_positions.length===1});
          // Save the actual collapsed frame before requiring a two-step header: a
          // native one-step merge is a failed observation, never a fabricated click.
          const collapsed=await waitFor(f=>observe(f).answer_positions.length===1 && unchanged(),
            'completed two-item reasoning answer');
          await shot('collapsed',collapsed);
          check('collapsed',collapsed,'collapsed');
          click('expand',await awaitState('before-expand-click','collapsed'));
          const expanded=await awaitState('expanded','expanded');
          check('expanded',expanded,'expanded');
          await shot('expanded',expanded);
          click('recollapse',await awaitState('before-recollapse-click','expanded'));
          const recollapsed=await awaitState('recollapsed','collapsed');
          check('recollapsed',recollapsed,'collapsed');
          await shot('recollapsed',recollapsed);
          lock.reasoning_steps[origin].status='PASS';save();
          lock.attempts.push({origin,status:'REASONING_STEPS_CHECKS_PASS',provider_counts:counts()});
        }
        if(autocomplete) {
         await probeAutocomplete('session');
         lock.autocomplete[origin].status='RECORDED';
         json('capture.lock.json',lock);
         lock.attempts.push({origin,status:'AUTOCOMPLETE_RECORDED',
           predicates:autocompleteChecks.map(c=>({route:c.route,stage:c.stage,...c.predicates}))});
       }
       if(autocompleteKeys) {
         await probeAutocompleteKeys('session');
         lock.autocomplete_keys[origin].status='PASS';
         json('capture.lock.json',lock);
         lock.attempts.push({origin,status:'AUTOCOMPLETE_KEYS_CHECKS_PASS',
           predicates:autocompleteKeysChecks.map(c=>({route:c.route,stage:c.stage,...c.predicates}))});
       }
        if(ctrlC) {
          await waitFor(f=>f.text.includes('GEOMETRY-SHORT: tool read completed.') &&
            logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length===1,
          'completed read and title before Ctrl+C session probe');
          const baseline=providerCounts();
          if(completedStatus!=='CAPTURED' || baseline.requests!==3 || baseline.completed!==3 || baseline.invalid!==0)
            throw Error('Ctrl+C session requires stable completed read/title and valid provider baseline');
          await probeCtrlC('session');
          lock.ctrl_c[origin].status='PASS';
          json('capture.lock.json',lock);
          lock.attempts.push({origin,status:'CTRL_C_CHECKS_PASS',
            predicates:ctrlCChecks.map(c=>({route:c.route,stage:c.stage,...c.predicates}))});
        }
        if(mention) {
          await probeMention('session');
          lock.mention[origin].status='RECORDED';
          json('capture.lock.json',lock);
          lock.attempts.push({origin,status:'MENTION_RECORDED',
            predicates:mentionChecks.map(c=>({route:c.route,stage:c.stage,...c.predicates}))});
        }
       if(sidebarPalette) {
         const checks=[];
         const expectedBefore=args.sidebar==='auto';
         const observe=f=>{
           const context=visibleMatches(f,'Context').filter(p=>p.x>=115 && p.y<20);
           const rightBackground=f.cells[10].at(-1).bg;
           // The Commands overlay dims the entire VT screen (#0a0a0a →
           // #040404); the underlying right pane must still be identified.
           const modal=f.text.includes('Commands');
           const visible=context.length===1 && (modal ? rightBackground!=='#040404' : rightBackground==='#141414');
           const hidden=context.length===0 && rightBackground===(modal?'#040404':'#0a0a0a');
           const labels=Object.fromEntries(['Show sidebar','Hide sidebar','Toggle sidebar'].map(s=>
             [s,visibleMatches(f,s).filter(p=>p.x>=50 && p.x<115 && p.y>=17 && p.y<40)]));
           return {context,right_background:rightBackground,modal,visible,hidden,labels};
         };
         lock.sidebar_palette ??= {};
         lock.sidebar_palette[origin]={status:'IN_PROGRESS',expected_initial:expectedBefore,checks};
         const record=(stage,f,passed,extra={})=>{
           checks.push({stage,passed,...observe(f),...extra});
           fs.writeFileSync(path.join(dir,'sidebar-palette-checks.json'),JSON.stringify(checks,null,2)+'\n');
           json('capture.lock.json',lock);
           if(!passed) throw Error('Sidebar palette predicate failed: '+stage);
         };
         const state=(f,visible)=>visible?observe(f).visible:observe(f).hidden;
         record('completed-session-sidebar',done,state(done,expectedBefore) && completedStatus==='CAPTURED');
         send('\x10','sidebar_palette_ctrl_p');
         const opened=await waitFor(f=>f.text.includes('Commands') && state(f,expectedBefore),'sidebar palette opened');
         record('palette-opened',opened,true);
         send('sidebar','sidebar_palette_search');
         // Observe the actual filtered palette; neither the upstream title nor
         // a native substitute is injected into the screen or fixture.
         const searched=await waitFor(f=>f.text.includes('Commands') &&
           visibleMatches(f,'sidebar').some(p=>p.y>10) && state(f,expectedBefore), 'sidebar search displayed');
         if(await capture('sidebar-palette-search',searched,'CAPTURED_SIDEBAR_SEARCH')!=='CAPTURED_SIDEBAR_SEARCH')
           throw Error('Unstable sidebar search');
         const found=observe(searched).labels;
         const painted=Object.entries(found).flatMap(([label,points])=>points.map(point=>({label,point})));
         const actual=painted.map(p=>p.label);
         const expectedLabel=expectedBefore?'Hide sidebar':'Show sidebar';
         // Absence is an observed result, not a fabricated expected label.
         record('search-result',searched,true,{actual_labels:actual,expected_label:expectedLabel,
           expected_label_present:actual.includes(expectedLabel),action_absent:actual.length===0});
         if(painted.length===1) {
           send('\r','sidebar_palette_select_return');
           const changed=await waitFor(f=>!f.text.includes('Commands') && state(f,!expectedBefore),
             'selected sidebar command changed visible state',12000);
           record('selected-action-effect',changed,true,{selected_label:actual[0],after_visible:!expectedBefore});
           if(await capture('sidebar-palette-after',changed,'CAPTURED_SIDEBAR_EFFECT')!=='CAPTURED_SIDEBAR_EFFECT')
             throw Error('Unstable sidebar action effect');
           lock.sidebar_palette[origin].status='ACTION_EFFECT_CONFIRMED';
         } else {
           lock.sidebar_palette[origin].status=painted.length===0?'ACTION_ABSENT':'AMBIGUOUS_RESULTS';
           if(painted.length>1) result=1;
         }
         json('capture.lock.json',lock);
         lock.attempts.push({origin,status:'SIDEBAR_PALETTE_'+lock.sidebar_palette[origin].status,
           initial_visible:expectedBefore,actual_labels:actual,expected_label:expectedLabel});
       }
         if(renameSession) {
          const checks=[];
          const checkFile=path.join(dir,'rename-checks.json');
          const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
            title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
            completed_transcript:logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length,
            completed_title:logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length,
            invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
           const baseline=counts();
           let expected=baseline;
           const noRequests=()=>canonical(counts())===canonical(expected);
          // The original fixture title is truncated by the tab width, whereas
          // the replacement fits: require its entire painted title on row 0.
           const namedTab=(f,name)=>visibleMatches(f,[renamedTitle,regeneratedTitle].includes(name) ? name : [...name].slice(0,6).join(''))
             .filter(p=>p.y===0);
           const observed=f=>({header:tabRow(f).trimEnd(),old_tabs:namedTab(f,tabTitle),
             renamed_tabs:namedTab(f,renamedTitle),regenerated_tabs:namedTab(f,regeneratedTitle),dialog:f.text.includes('Rename session'),
            prefilled:visibleMatches(f,tabTitle).some(p=>p.y>0),
            renamed_value:visibleMatches(f,renamedTitle).some(p=>p.y>0),
            transcript:f.text.includes('GEOMETRY-SHORT: tool read completed.'),
            home:f.text.includes('Ask anything') && f.text.includes('Reader · MiMo-V2.6-Flash Free')});
          lock.rename_interactions ??= {};
          lock.rename_interactions[origin]={status:'IN_PROGRESS',baseline,checks};
          const save=()=>{json(path.relative(output,checkFile),{baseline,checks});json('capture.lock.json',lock);};
          const check=(stage,f,predicate,passed,extra={})=>{
            checks.push({stage,predicate,passed,...observed(f),provider_counts:counts(),...extra});
            save();
            if(!passed) throw Error('Rename predicate failed: '+stage);
          };
          const awaitRename=async(stage,predicate,description)=>{
            let f;
            try {f=await waitFor(predicate,description);}
            catch(e) {check(stage,await frame(),description+': '+e.message,false);}
            check(stage,f,description,predicate(f));
            return f;
          };
          const before=f=>{const o=observed(f);return o.old_tabs.length===1 && o.renamed_tabs.length===0 &&
            o.transcript && !o.dialog && !o.home && noRequests();};
          const ready=await awaitRename('completed-and-titled',before,'one fixture-titled tab, completed read, no modal or new requests');
          check('provider-baseline',ready,'two completed transcript requests and one completed title request, valid fixture contract',
            baseline.transcript===2 && baseline.title===1 && baseline.completed_transcript===2 &&
            baseline.completed_title===1 && baseline.invalid===0,{baseline});
          send('\x12','rename_ctrl_r');
          const dialog=f=>{const o=observed(f);return o.dialog && o.prefilled && o.old_tabs.length===1 &&
            o.transcript && noRequests();};
          const prefilled=await awaitRename('dialog-prefilled',dialog,'Rename session dialog with actual fixture title prefilled, no provider requests');
          if(await capture('rename-prefilled',prefilled,'CAPTURED_RENAME_PREFILLED')!=='CAPTURED_RENAME_PREFILLED')
            throw Error('Unstable rename-prefilled frame');
          // Both modal textareas accept Home followed by Shift+End replacement;
          // Ctrl+A is editor-specific (often moves to line start), not select-all.
          send('\x1b[H','rename_home');
          await sleep(100);
          send('\x1b[1;2F','rename_shift_end');
          await sleep(100);
          send(renamedTitle,'rename_keyboard_replacement');
          const edited=f=>{const o=observed(f);return o.dialog && o.renamed_value && !o.prefilled &&
            o.old_tabs.length===1 && noRequests();};
          const draft=await awaitRename('dialog-edited',edited,'modal input displays replacement title, original prefill absent, no provider requests');
          if(await capture('rename-edited',draft,'CAPTURED_RENAME_EDITED')!=='CAPTURED_RENAME_EDITED')
            throw Error('Unstable rename-edited frame');
          send('\r','rename_submit_return');
          const renamed=f=>{const o=observed(f);return !o.dialog && o.renamed_tabs.length===1 &&
            o.old_tabs.length===0 && o.transcript && !o.home && noRequests();};
          const after=await awaitRename('renamed-in-session',renamed,'renamed tab/header with completed transcript, original title gone, no provider requests');
          if(await capture('rename-after',after,'CAPTURED_RENAMED')!=='CAPTURED_RENAMED')
            throw Error('Unstable rename-after frame');
           const verified=await frame();
           let prequitFrame=verified;
           check('rename-capture-verified',verified,'renamed tab and transcript persist without provider requests',renamed(verified));
           if(regenerateTitle) {
             check('before-regeneration',verified,'manual title unique, original and regenerated titles absent; one genuine completed title',
               renamed(verified) && observed(verified).regenerated_tabs.length===0 &&
               baseline.completed_title===1 && baseline.title===1);
             send('/rename','regenerate_bare_slash_command');
             // Upstream autocomplete contains the label "Rename session" while
             // /rename is being typed; it is a suggestion, not the rename modal.
             const commandDraft=f=>visibleMatches(f,'/rename').some(p=>p.y>=30) &&
               namedTab(f,renamedTitle).length===1 && namedTab(f,tabTitle).length===0 &&
               observed(f).transcript && noRequests();
             const slashDraft=await awaitRename('regenerate-command-draft',commandDraft,
               'bare /rename in composer with manual title still present, no new provider requests');
             check('regenerate-command-before-submit',slashDraft,'manual tab and composer slash visible, exactly baseline requests',commandDraft(slashDraft));
             // The pinned command editor accepts Enter as autocomplete while
             // suggestions are visible. Escape closes only autocomplete and
             // retains the slash draft (pinned app-lifecycle.test.tsx:552-555).
             if(origin==='upstream') {
               send('\x1b','regenerate_dismiss_suggestions_escape');
               await awaitRename('regenerate-suggestions-dismissed',f=>commandDraft(f) && !f.text.includes('/review'),
                 'bare /rename still in composer after dismissing upstream autocomplete');
             }
             send('\r','regenerate_submit_return');
             const regenerated=f=>{const o=observed(f),c=counts();return !o.dialog && o.regenerated_tabs.length===1 &&
               o.renamed_tabs.length===0 && o.old_tabs.length===0 && o.transcript && !o.home &&
               c.transcript===baseline.transcript && c.completed_transcript===baseline.completed_transcript &&
               c.title===baseline.title+1 && c.completed_title===baseline.completed_title+1 && c.invalid===0 &&
               logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').at(-1)?.response_text_sha256===sha(regeneratedTitle);};
             const generated=await awaitRename('regenerated-in-session',regenerated,
               'distinct provider-completed title on unique painted tab, one additional title and zero transcript requests');
             if(await capture('rename-regenerated',generated,'CAPTURED_REGENERATED')!=='CAPTURED_REGENERATED')
               throw Error('Unstable rename-regenerated frame');
             expected=counts();
             const afterGeneration=await frame();
             check('regeneration-capture-verified',afterGeneration,
               'genuine regenerated title remains painted; provider counts stop at exactly two titles',
               regenerated(afterGeneration) && noRequests(),{expected});
             prequitFrame=afterGeneration;
           }
           const finalTitle=regenerateTitle ? regeneratedTitle : renamedTitle;
           const current=f=>{const o=observed(f);return !o.dialog && namedTab(f,finalTitle).length===1 &&
             o.old_tabs.length===0 && (regenerateTitle ? o.renamed_tabs.length===0 : o.regenerated_tabs.length===0) &&
             o.transcript && !o.home && noRequests();};
           send('\x04','rename_graceful_exit_ctrl_d');
          const deadline=Date.now()+15000;
          while(!logs.some(e=>e.kind==='exit' && e.generation===0) && Date.now()<deadline) await sleep(100);
          const exit=logs.find(e=>e.kind==='exit' && e.generation===0);
           check('graceful-exit',prequitFrame,'first process exited naturally with code 0; no provider requests',
            !!exit && exit.code===0 && exit.termination==='natural' && noRequests(),{exit});
          fs.writeFileSync(path.join(dir,'prequit.raw.vt'),Buffer.concat(chunks[0]));
          fs.writeFileSync(path.join(dir,'prequit.protocol.json'),JSON.stringify(logs,null,2)+'\n');
          fs.writeFileSync(path.join(dir,'prequit.inputs.json'),JSON.stringify(inputs,null,2)+'\n');
          prequitBoundary={logs:logs.length,inputs:inputs.length};
          generation=1;
          await writeQueue;
          await page.evaluate(()=>term.reset());
          child.stdin.write(JSON.stringify({kind:'relaunch'})+'\n');
           const restoredHome=f=>{const o=observed(f);return o.home && namedTab(f,finalTitle).length===1 &&
             o.old_tabs.length===0 && (regenerateTitle ? o.renamed_tabs.length===0 : o.regenerated_tabs.length===0) &&
             !o.dialog && !o.transcript && noRequests() &&
             logs.some(e=>e.kind==='relaunch' && e.generation===1);};
           const home=await awaitRename('restored-home',restoredHome,'Home with persisted final title, no transcript or provider requests');
           if(await capture(regenerateTitle?'rename-regenerated-restored-home':'rename-restored-home',home,'CAPTURED_RENAME_RESTORED_HOME')!=='CAPTURED_RENAME_RESTORED_HOME')
             throw Error('Unstable rename-restored-home frame');
           const target=namedTab(await frame(),finalTitle);
           check('restored-tab-target',await frame(),'one painted final-title tab on Home and no provider requests',target.length===1 && noRequests(),{target:target[0]});
          const point=target[0],down=`\x1b[<0;${point.x+1};${point.y+1}M`,up=`\x1b[<0;${point.x+1};${point.y+1}m`;
          checks.push({stage:'restored-tab-click',point,down_base64:Buffer.from(down).toString('base64'),
            up_base64:Buffer.from(up).toString('base64')});save();
          send(down,'restored_renamed_tab_mouse_down');send(up,'restored_renamed_tab_mouse_up');
           const replay=await awaitRename('restored-renamed-session',current,'final-title tab selected from Home, durable transcript visible, no provider requests');
           if(await capture(regenerateTitle?'rename-regenerated-restored-session':'rename-restored-session',replay,'CAPTURED_RENAME_RESTORED_SESSION')!=='CAPTURED_RENAME_RESTORED_SESSION')
             throw Error('Unstable rename-restored-session frame');
           const afterReplay=await frame();
           check('post-capture-no-requests',afterReplay,'final title and transcript persist after restart and click, no additional provider requests',
             current(afterReplay) && noRequests(),{expected});
           lock.rename_interactions[origin].status='PASS';save();
           lock.attempts.push({origin,status:regenerateTitle?'REGENERATE_TITLE_CHECKS_PASS':'RENAME_CHECKS_PASS',baseline,after:counts(),
            predicates:checks.filter(c=>c.passed).map(c=>c.stage)});
        }
        if(tabClick) {
         if(completedStatus !== 'CAPTURED') throw Error('Tab interaction requires a stable completed-session capture');
         const checks = [];
         const checkFile = path.join(dir,'tab-checks.json');
         lock.tab_interactions ??= {};
         lock.tab_interactions[origin] = {status:'IN_PROGRESS',checks};
         const saveChecks = () => {
           fs.writeFileSync(checkFile,JSON.stringify(checks,null,2)+'\n');
           json('capture.lock.json',lock);
         };
         const record = (stage, f, predicate, passed, extra={}) => {
           checks.push({stage,predicate,passed,...tabObserve(f),...extra});
           saveChecks();
           if(!passed) throw Error('Tab predicate failed: '+stage);
         };
         const readyPredicate = c => c.add.length===1 && c.old.length===1 && c.new_title.length===0 &&
           c.add[0].x>c.old[0].x && c.old_content && !c.home_prompt &&
           logs.some(e=>e.kind==='provider_completed' && e.operation==='transcript');
         let old;
         try { old=await waitFor(f=>readyPredicate(tabObserve(f)),'painted completed original tab and add control'); }
         catch(e) { record('completed-before-add',await frame(),'painted completed old tab and +: '+e.message,false); }
         const before = tabObserve(old);
         record('completed-before-add',old,'unique painted row-0 + after original tab, completed old transcript, no Home',readyPredicate(before));
         const click = (stage, point) => {
           // SGR PTY coordinates are one-based. Original's add/select handlers
           // activate on release; the bridge consumes JSONL inputs asynchronously.
           const column=point.x+1, row=point.y+1;
           const down=`\x1b[<0;${column};${row}M`, up=`\x1b[<0;${column};${row}m`;
           checks.push({stage:stage+'-click',point,pty_column:column,pty_row:row,
             down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});
           saveChecks();
           send(down,stage+'_mouse_down'); send(up,stage+'_mouse_up');
         };
         click('add',{x:before.add[0].x+1,y:0});
         const addedPredicate = c => c.old.length===1 && c.new_title.length===1 && c.add.length===0 &&
           c.old[0].x<c.new_title[0].x && c.home_prompt && !c.old_content;
         let added;
         try { added=await waitFor(f=>addedPredicate(tabObserve(f)),'Home with retained old tab and + New session'); }
         catch(e) { record('tab-added',await frame(),'old tab + synthetic New session on Home, old transcript absent: '+e.message,false); }
         record('tab-added',added,'old tab + synthetic New session on Home, old transcript absent',addedPredicate(tabObserve(added)));
          if(await capture('tab-added',added,'CAPTURED_TAB_ADDED') !== 'CAPTURED_TAB_ADDED')
            throw Error('Unstable tab-added frame');
           if(tabRestart) {
             const checks=[], file=path.join(dir,'tab-restart-checks.json');
             const counts=()=>({transcripts:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
               titles:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
               invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
             lock.tab_restart_interactions ??= {};
             lock.tab_restart_interactions[origin]={status:'IN_PROGRESS',checks};
             const check=(stage,f,passed,extra={})=>{
               checks.push({stage,passed,...restartObserve(f),counts:counts(),...extra});
               fs.writeFileSync(file,JSON.stringify(checks,null,2)+'\n');
               json('capture.lock.json',lock);
               if(!passed) throw Error('Tab restart predicate failed: '+stage);
             };
             const pointClick=(stage,p)=>{
               const down=`\x1b[<0;${p.x+1};${p.y+1}M`,up=`\x1b[<0;${p.x+1};${p.y+1}m`;
               checks.push({stage:stage+'-click',point:p,down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});
               fs.writeFileSync(file,JSON.stringify(checks,null,2)+'\n');
               send(down,stage+'_mouse_down');send(up,stage+'_mouse_up');
             };
             send('\x1b[200~'+fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim()+'\x1b[201~','second_prompt_paste');
             await sleep(200);send('\r','second_submit');
             const secondReady=f=>{const c=restartObserve(f);return c.old.length===1 && c.second.length===1 && c.add.length===1 &&
               c.old[0].x<c.second[0].x && c.second[0].x<c.add[0].x && c.second_content && !c.old_content &&
                counts().transcripts===4 && counts().titles===2 && counts().invalid===0;};
             const second=await waitFor(secondReady,'second real read transcript and two ordered tabs');
             check('second-real-session',second,secondReady(second));
             await capture('tab-second-completed',second,'CAPTURED_SECOND_REAL_SESSION');
             pointClick('select-old',restartObserve(await frame()).old[0]);
             const oldReady=f=>{const c=restartObserve(f);return c.old.length===1 && c.second.length===1 && c.add.length===1 &&
               c.old[0].x<c.second[0].x && c.second[0].x<c.add[0].x && c.old_content && !c.second_content;};
             const prequit=await waitFor(oldReady,'old selected with second real tab');
             check('prequit-old-selected',prequit,oldReady(prequit));
             await capture('tab-prequit-old',prequit,'CAPTURED_PREQUIT');
             const baseline=counts();
             check('two-completed-sessions',prequit,baseline.transcripts===4 && baseline.titles===2 &&
               logs.filter(e=>e.kind==='provider_completed' && e.operation==='transcript').length===4 &&
               logs.filter(e=>e.kind==='provider_completed' && e.operation==='title').length===2 && baseline.invalid===0,
               {baseline});
             // app.exit: ctrl+c,ctrl+d,<leader>q (pinned packages/tui/src/config/keybind.ts:48).
             // Native app.exit also accepts Ctrl+D at an empty idle composer.
             send('\x04','graceful_app_exit_ctrl_d');
             const deadline=Date.now()+15000;
             while(!logs.some(e=>e.kind==='exit' && e.generation===0) && Date.now()<deadline) await sleep(100);
             const exit=logs.find(e=>e.kind==='exit' && e.generation===0);
             check('graceful-exit',prequit,!!exit && exit.code===0 && exit.termination==='natural',
               {exit,baseline,source:'pinned packages/tui/src/config/keybind.ts:48; native crates/oc-tui/src/events.rs'});
             fs.writeFileSync(path.join(dir,'prequit.raw.vt'),Buffer.concat(chunks[0]));
             fs.writeFileSync(path.join(dir,'prequit.protocol.json'),JSON.stringify(logs,null,2)+'\n');
             fs.writeFileSync(path.join(dir,'prequit.inputs.json'),JSON.stringify(inputs,null,2)+'\n');
             prequitBoundary={logs:logs.length,inputs:inputs.length};
             generation=1;
             await writeQueue;
             await page.evaluate(()=>term.reset());
             child.stdin.write(JSON.stringify({kind:'relaunch'})+'\n');
             const noRequests=()=>JSON.stringify(counts())===JSON.stringify(baseline);
             const restored=await waitFor(f=>{const c=restartObserve(f);return c.old.length===1 && c.second.length===1 &&
               c.old[0].x<c.second[0].x && ((oldReady(f)) || (c.new_title.length===1 && c.home_prompt &&
                 !c.old_content && !c.second_content)) && noRequests() &&
               logs.some(e=>e.kind==='relaunch' && e.generation===1);},
               'restored ordered real tabs and observed selection without provider requests');
             const selectedOld=oldReady(restored);
              // Standalone v2.0.12 starts at Home when no initial route is
              // supplied (context/route.tsx), even with stored real tabs.
              const selectedHome=!selectedOld && restartObserve(restored).new_title.length===1;
              checks.push({stage:'restored-entry-selection',passed:selectedHome,observed:selectedOld?'old':'synthetic_home',
               ...restartObserve(restored),baseline,counts:counts()});
             fs.writeFileSync(file,JSON.stringify(checks,null,2)+'\n');
             lock.tab_restart_interactions[origin].selection=selectedOld?'old':'synthetic_home';
             json('capture.lock.json',lock);
             await capture('tab-restored-entry',restored,'CAPTURED_RESTORED_ENTRY');
             if(!selectedOld) pointClick('restored-select-old',restartObserve(await frame()).old[0]);
             const restoredOld=selectedOld?restored:await waitFor(f=>oldReady(f) && noRequests(),
               'old history restored by real click without provider requests');
             check('restored-old-history',restoredOld,oldReady(restoredOld) && noRequests(),{baseline});
             await capture('tab-restored-old',restoredOld,'CAPTURED_RESTORED_OLD');
             pointClick('restored-select-second',restartObserve(await frame()).second[0]);
             const other=await waitFor(f=>{const c=restartObserve(f);return c.second_content && !c.old_content &&
               c.old.length===1 && c.second.length===1 && c.add.length===1 &&
               c.old[0].x<c.second[0].x && c.second[0].x<c.add[0].x && JSON.stringify(counts())===JSON.stringify(baseline);},
               'restored second history via real click, no provider requests');
             check('restored-second-click',other,JSON.stringify(counts())===JSON.stringify(baseline),{baseline});
             await capture('tab-restored-second',other,'CAPTURED_RESTORED_SECOND');
             check('post-capture-no-requests',await frame(),noRequests(),{baseline});
              lock.tab_restart_interactions[origin].status=selectedHome?'PASS':'DIFFERENT_SELECTION';
              json('capture.lock.json',lock);
              lock.attempts.push({origin,status:selectedHome?'TAB_RESTART_CHECKS_PASS':'TAB_RESTART_SELECTION_DIFFERENT',
                selected_on_relaunch:selectedOld?'old':'synthetic_home',predicates:checks.filter(c=>c.passed).map(c=>c.stage),baseline,after:counts()});
              if(!selectedHome) result=1;
            } else if(tabCloseKey) {
             const keyChecks = [];
             const counts = () => ({provider_requests:logs.filter(e=>e.kind==='provider').length,
               provider_completed:logs.filter(e=>e.kind==='provider_completed').length,
               transcript_requests:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
               title_requests:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
               invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
             const baseline = counts();
             lock.tab_close_key_interactions ??= {};
             lock.tab_close_key_interactions[origin] = {status:'IN_PROGRESS',baseline,checks:keyChecks};
             const saveKey = () => {
               fs.writeFileSync(path.join(dir,'tab-close-key-checks.json'),JSON.stringify({baseline,checks:keyChecks},null,2)+'\n');
               json('capture.lock.json',lock);
             };
             const keyRecord = (stage, f, predicate, passed, extra={}) => {
               keyChecks.push({stage,predicate,passed,...tabObserve(f),provider_counts:counts(),...extra});
               saveKey();
               if(!passed) throw Error('Tab keyboard close predicate failed: '+stage);
             };
             const noCall = () => JSON.stringify(counts())===JSON.stringify(baseline);
             const beforeKey = await frame();
             keyRecord('before-keyboard-close',beforeKey,
               'old tab and synthetic Home selected, old transcript absent, provider counts unchanged',
               addedPredicate(tabObserve(beforeKey)) && noCall());
             // Real PTY leader chord, as in the dialog key path: send the
             // Ctrl+X prefix and then w, rather than changing the fixture.
             const leader='\x18', key='w';
             keyChecks.push({stage:'keyboard-close-input',leader_base64:Buffer.from(leader).toString('base64'),
               key_base64:Buffer.from(key).toString('base64'),provider_counts:counts()});
             saveKey();
             send(leader,'home_close_ctrl_x');
             await sleep(100);
             send(key,'home_close_w');
             const closedPredicate = f => {
               const c=tabObserve(f);
               return c.old.length===1 && c.new_title.length===0 && c.add.length===1 &&
                 c.add[0].x>c.old[0].x && c.old_content && !c.home_prompt && noCall();
             };
             let closedFrame;
             try { closedFrame=await waitFor(closedPredicate,'keyboard-closed synthetic Home and restored old transcript without provider requests'); }
             catch(e) { keyRecord('keyboard-closed',await frame(),'old transcript restored, synthetic Home gone, provider counts unchanged: '+e.message,false); }
             keyRecord('keyboard-closed',closedFrame,'old transcript restored, synthetic Home gone, provider counts unchanged',
               closedPredicate(closedFrame));
             if(await capture('keyboard-close-after',closedFrame,'CAPTURED_TAB_KEYBOARD_CLOSED') !== 'CAPTURED_TAB_KEYBOARD_CLOSED')
               throw Error('Unstable keyboard-close frame');
             const afterCapture=await frame();
             keyRecord('keyboard-close-capture-verified',afterCapture,
               'old transcript and synthetic removal persist, provider counts unchanged',closedPredicate(afterCapture));
             lock.tab_close_key_interactions[origin].status='PASS';
             saveKey();
             lock.attempts.push({origin,status:'TAB_CLOSE_KEY_CHECKS_PASS',provider_counts:counts(),
               predicates:keyChecks.filter(c=>c.passed===true).map(c=>c.stage)});
            } else if(tabClose) {
            const closeChecks = [];
            const counts = () => ({provider_requests:logs.filter(e=>e.kind==='provider').length,
              provider_completed:logs.filter(e=>e.kind==='provider_completed').length,
              transcript_requests:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
              title_requests:logs.filter(e=>e.kind==='provider' && e.operation==='title').length});
            const baseline = counts();
            lock.tab_close_interactions ??= {};
            lock.tab_close_interactions[origin] = {status:'IN_PROGRESS',baseline,checks:closeChecks};
            const saveClose = () => {
              fs.writeFileSync(path.join(dir,'tab-close-checks.json'),JSON.stringify({baseline,checks:closeChecks},null,2)+'\n');
              json('capture.lock.json',lock);
            };
            const closeRecord = (stage, f, predicate, passed, extra={}) => {
              closeChecks.push({stage,predicate,passed,...tabObserve(f),provider_counts:counts(),...extra});
              saveClose();
              if(!passed) throw Error('Tab close predicate failed: '+stage);
            };
            const noCall = () => JSON.stringify(counts())===JSON.stringify(baseline);
            saveClose();
            // Move onto the painted synthetic title; SGR 35 is a motion event
            // (not a click). Each application must reveal its own close cell.
            const home=tabObserve(added).new_title[0];
            const hover={x:home.x+3,y:home.y};
            const move=`\x1b[<35;${hover.x+1};${hover.y+1}M`;
            closeChecks.push({stage:'home-hover-move',point:hover,pty_column:hover.x+1,pty_row:hover.y+1,
              move_base64:Buffer.from(move).toString('base64'),provider_counts:counts()});
            saveClose();
            send(move,'home_close_mouse_move');
            const closeGlyphs = f => {
              const c=tabObserve(f);
              return c.new_title.length===1 ? visibleMatches(f,'✕').filter(p=>p.y===0 && p.x>c.new_title[0].x) : [];
            };
            const hoveredPredicate = f => addedPredicate(tabObserve(f)) && closeGlyphs(f).length===1 && noCall();
            let hovered;
            try { hovered=await waitFor(hoveredPredicate,'visible hovered synthetic Home close glyph'); }
            catch(e) { closeRecord('home-hovered',await frame(),'unique visible row-0 ✕ after synthetic title: '+e.message,false); }
            const glyphs=closeGlyphs(hovered);
            closeRecord('home-hovered',hovered,'one visible ✕ after the synthetic title, Home selected, no new provider request',
              hoveredPredicate(hovered),{hover,glyph:glyphs[0]});
            if(await capture('tab-close-hovered-before',hovered,'CAPTURED_TAB_CLOSE_HOVERED') !== 'CAPTURED_TAB_CLOSE_HOVERED')
              throw Error('Unstable hovered tab-close frame');
            // Re-measure after screenshot; never reuse the upstream x for Rust.
            const beforeClose=await frame();
            const target=closeGlyphs(beforeClose);
            closeRecord('before-close',beforeClose,'one still-visible synthetic ✕, no provider call',
              addedPredicate(tabObserve(beforeClose)) && target.length===1 && noCall(),{glyph:target[0]});
            const point=target[0];
            const column=point.x+1, row=point.y+1;
            const down=`\x1b[<0;${column};${row}M`, up=`\x1b[<0;${column};${row}m`;
            closeChecks.push({stage:'home-close-click',point,pty_column:column,pty_row:row,
              down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64'),
              provider_counts:counts()});
            saveClose();
            send(down,'home_close_mouse_down'); send(up,'home_close_mouse_up');
            const closedPredicate = f => {
              const c=tabObserve(f);
              return c.old.length===1 && c.new_title.length===0 && c.add.length===1 &&
                c.add[0].x>c.old[0].x && c.old_content && !c.home_prompt &&
                closeGlyphs(f).length===0 && noCall();
            };
            let closedFrame;
            try { closedFrame=await waitFor(closedPredicate,'synthetic Home closed and old transcript restored without provider call'); }
            catch(e) { closeRecord('home-closed',await frame(),'old transcript restored, synthetic slot gone, provider counts unchanged: '+e.message,false); }
            closeRecord('home-closed',closedFrame,'old transcript restored, synthetic slot gone, provider counts unchanged',
              closedPredicate(closedFrame));
            if(await capture('tab-close-closed-after',closedFrame,'CAPTURED_TAB_CLOSE_CLOSED') !== 'CAPTURED_TAB_CLOSE_CLOSED')
              throw Error('Unstable closed tab-close frame');
            const afterCapture=await frame();
            closeRecord('closed-capture-verified',afterCapture,'old transcript and synthetic removal persist, provider counts unchanged',
              closedPredicate(afterCapture));
            lock.tab_close_interactions[origin].status='PASS';
            saveClose();
            lock.attempts.push({origin,status:'TAB_CLOSE_CHECKS_PASS',provider_counts:counts(),
              predicates:closeChecks.filter(c=>c.passed===true).map(c=>c.stage)});
           } else {
            const beforeReturn=await frame();
            const returning=tabObserve(beforeReturn);
            record('before-return',beforeReturn,'retained old tab uniquely painted on Home',addedPredicate(returning));
            // Click inside the old title, not the synthetic Home slot. Re-locate
            // on this side after the asynchronous route transition and screenshot.
            click('return',returning.old[0]);
            const returnedPredicate = c => c.old.length===1 && c.new_title.length===0 && c.add.length===1 &&
              c.add[0].x>c.old[0].x && c.old_content && !c.home_prompt;
            let returned;
            try { returned=await waitFor(f=>returnedPredicate(tabObserve(f)),'returned original session transcript'); }
            catch(e) { record('tab-returned',await frame(),'old transcript restored with original tab selected: '+e.message,false); }
            record('tab-returned',returned,'old transcript restored with original tab selected',returnedPredicate(tabObserve(returned)));
            if(await capture('tab-returned',returned,'CAPTURED_TAB_RETURNED') !== 'CAPTURED_TAB_RETURNED')
              throw Error('Unstable tab-returned frame');
          }
         lock.tab_interactions[origin].status='PASS';
         json('capture.lock.json',lock);
         lock.attempts.push({origin,status:'TAB_INTERACTION_CHECKS_PASS',predicates:checks.filter(c=>c.passed===true).map(c=>c.stage)});
       }
      if(explorationClick) {
         const checks = [];
         const headerText = '→ Explored — 1 read';
         const detailText = 'Read fixture-note.txt';
         const observe = f => {
           const headers=visibleMatches(f,headerText), details=visibleMatches(f,detailText);
           return {header_count:headers.length, detail_count:details.length,
             header:headers.length===1?headers[0]:null,
             detail_below_header:headers.length===1 && details.length===1 && details[0].y>headers[0].y};
         };
         const record = (stage, f, predicate, passed) => {
           const check={stage, predicate, passed, ...observe(f)};
           checks.push(check);
           fs.writeFileSync(path.join(dir,'exploration-checks.json'),JSON.stringify(checks,null,2)+'\n');
           if(!passed) throw Error('Exploration predicate failed: '+stage);
           return check;
         };
         const awaitState = async (stage, predicate, description) => {
           let f;
           try { f=await waitFor(f=>predicate(observe(f)),description); }
           catch(e) {
             const current=await frame();
             checks.push({stage,predicate:description,passed:false,reason:e.message,...observe(current)});
             fs.writeFileSync(path.join(dir,'exploration-checks.json'),JSON.stringify(checks,null,2)+'\n');
             throw e;
           }
           record(stage,f,description,predicate(observe(f)));
           return f;
         };
         const collapsed = await awaitState('collapsed', c=>c.header_count===1 && c.detail_count===0,
           'one visible → Explored — 1 read header and no Read fixture-note.txt detail');
         const click = (stage, f) => {
           // Aim inside "Explored", retaining one-based SGR PTY coordinates.
           const {x,y}=observe(f).header;
           const column=x+3, row=y+1;
           const down=`\x1b[<0;${column};${row}M`, up=`\x1b[<0;${column};${row}m`;
           checks.push({stage:stage+'-click',header:{x,y},pty_column:column,pty_row:row,
             down_base64:Buffer.from(down).toString('base64'),up_base64:Buffer.from(up).toString('base64')});
           fs.writeFileSync(path.join(dir,'exploration-checks.json'),JSON.stringify(checks,null,2)+'\n');
           send(down,stage+'_mouse_down'); send(up,stage+'_mouse_up');
         };
         click('expand',collapsed);
         const expanded=await awaitState('expanded',c=>c.header_count===1 && c.detail_below_header,
           'collapsed header remains visible and Read fixture-note.txt detail appears below it');
         await capture('exploration-expanded',expanded,'CAPTURED_EXPLORATION_EXPANDED');
         const beforeRecollapse=await frame();
         const visible=observe(beforeRecollapse);
         record('expanded-before-recollapse',beforeRecollapse,
           'one visible collapsed header and Read fixture-note.txt detail below it',
           visible.header_count===1 && visible.detail_below_header);
         click('recollapse',beforeRecollapse);
         const recollapsed=await awaitState('recollapsed',c=>c.header_count===1 && c.detail_count===0,
           'collapsed header remains visible and Read fixture-note.txt detail disappears');
         await capture('exploration-recollapsed',recollapsed,'CAPTURED_EXPLORATION_RECOLLAPSED');
      }
      if(args.tabs==='vertical') {
        const sidebarAbsent = !done.text.includes('Context') && done.cells[10].at(-1).bg==='#0a0a0a';
        const sidebarPresent = done.text.includes('Context') && done.cells[10].at(-1).bg==='#141414';
        const expectedSidebar = profile.settings.sidebar!=='hide' && profile.columns-42>120;
        fs.writeFileSync(path.join(dir,'chrome-checks.json'),JSON.stringify({tabs:'vertical',columns:profile.columns,expected_sidebar:expectedSidebar,sidebar_absent:sidebarAbsent,sidebar_present:sidebarPresent,right_background:done.cells[10].at(-1).bg},null,2)+'\n');
        if(expectedSidebar ? !sidebarPresent : !sidebarAbsent) throw Error('vertical rail styled sidebar breakpoint differs');
      }
      if(args['scroll-resize'] === 'true') {
        const checks = [];
        const markers = f => [...f.text.matchAll(/ROW-(\d+)/g)].map(m=>Number(m[1]));
        const reflow = args.sample === 'rows-reflow';
        const firstVisibleRow = f => {
          for (const [y, cells] of f.cells.entries()) {
            const row = cells.map(c=>c.symbol).join('');
            const match = /ROW-(\d{3})/.exec(row);
            if (match) return {marker:match[0], index:Number(match[1]), y};
          }
          return null;
        };
        const check = async (name, predicate) => {
          const f = await waitFor(predicate, name);
          await capture(name, f, 'CAPTURED_SCROLL_GEOMETRY');
          const draftRow = f.cells.findIndex(row=>row.map(c=>c.symbol).join('').includes('scroll-draft'));
          const rowText = draftRow < 0 ? '' : f.cells[draftRow].map(c=>c.symbol).join('');
          const draftX = rowText.indexOf('scroll-draft');
          const c = {scenario:name, columns:f.columns, rows:f.rows, markers:markers(f),
            first_visible_row:firstVisibleRow(f), cursor:f.cursor, draft_row:draftRow,
            cursor_at_draft_end:f.cursor.visible && f.cursor.x===draftX+12 && f.cursor.y===draftRow};
          checks.push(c);
          fs.writeFileSync(path.join(dir,'scroll-checks.json'),JSON.stringify(checks,null,2)+'\n');
          if(!c.cursor_at_draft_end) throw Error('draft/cursor lost: '+name);
          return c;
        };
        send('scroll-draft','single_line_draft');
        await check('scroll-pinned-draft',f=>f.text.includes('scroll-draft') && f.text.includes('ROW-089'));
        // Up/Down are editor/history keys in the native prompt; wheel input
        // targets the transcript without replacing the draft with a recalled
        // user message. The original retains its documented scroll binding.
        send((origin==='oc' ? '\x1b[<64;10;15M' : '\x1b\x19').repeat(12),'scroll_away_12_lines');
        const away = await check('scroll-away',f=>f.text.includes('scroll-draft') && markers(f).length>0 && !f.text.includes('ROW-089'));
        const resizeAnchors = {away:away.first_visible_row};
        let grown;
        for(const [columns,rows,name] of [[80,24,'scroll-shrink'],[160,48,'scroll-grow']]) {
          profile.columns=columns; profile.rows=rows;
          await page.evaluate(({columns,rows})=>term.resize(columns,rows),{columns,rows});
          child.stdin.write(JSON.stringify({kind:'resize',columns,rows})+'\n');
          const c = await check(name,f=>f.columns===columns && f.rows===rows && f.text.includes('scroll-draft') &&
            markers(f).length>0 && (reflow || !f.text.includes('ROW-089')));
          resizeAnchors[name==='scroll-shrink'?'shrink':'grow'] = c.first_visible_row;
          if(name==='scroll-grow') grown=c;
          if(!reflow && c.markers[0]!==away.markers[0]) throw Error('scroll top row lost during resize: '+name);
        }
        fs.writeFileSync(path.join(dir,'scroll-resize-anchors.json'),JSON.stringify({sample:spec.sample,
          origin, anchors:resizeAnchors},null,2)+'\n');
        send(origin==='oc' ? '\x1b[<65;10;15M' : '\x1b\x05','scroll_down_one');
        // This fixture grows back to the short rows after ROW-041. One scroll
        // must advance that visible marker; accepting any stable frame would
        // mistakenly pass if the Down event were lost.
        const down = await check('scroll-down-one',f=>reflow
          ? f.text.includes('scroll-draft') && firstVisibleRow(f)?.index===grown.first_visible_row.index+1
          : markers(f).at(-1)===away.markers.at(-1)+1);
        send((origin==='oc' ? '\x1b[<65;10;15M' : '\x1b\x05').repeat(100),'scroll_repin');
        await check('scroll-repinned',f=>f.text.includes('ROW-089') && f.text.includes('scroll-draft'));
        lock.attempts.push({origin,status:'SCROLL_RESIZE_CHECKS_PASS',one_line_down:down.markers.at(-1)});
      }
      if(args.matrix === 'true') {
        const checks = [];
        for(const [columns, rows] of [[80,24],[120,40],[160,48],[43,48],[44,48],[119,48],[120,48],[121,48],[120,80],[160,48]]) {
          profile.columns=columns; profile.rows=rows;
          await page.evaluate(({columns,rows}) => term.resize(columns,rows), {columns,rows});
          child.stdin.write(JSON.stringify({kind:'resize',columns,rows})+'\n');
          const f = await waitFor(f=>f.columns===columns && f.rows===rows && /MiMo-V2.6-Flash Free/.test(f.text), 'resize '+columns+'x'+rows);
          const name = 'resize-'+columns+'x'+rows+'-'+checks.length;
          await capture(name, f, 'CAPTURED');
          const count = (f.text.match(/ROW-\d+/g)||[]).length;
          const draftRow = f.cells.findIndex(row=>row.map(c=>c.symbol).join('').includes('scroll-draft'));
          const draftText = draftRow < 0 ? '' : f.cells[draftRow].map(c=>c.symbol).join('');
          const draftX = draftText.indexOf('scroll-draft');
          const draftCursor = f.cursor.visible && f.cursor.x===draftX+12 && f.cursor.y===draftRow;
          checks.push({scenario:name, columns, rows, visible_row_markers:count,
             viewport_check:args.sample==='rows' && rows>=40 ? (count>20?'PASS':'FAIL') : 'NOT_APPLICABLE',
            ...(args['scroll-resize']==='true' ? {draft_row:draftRow,cursor_at_draft_end:draftCursor} : {}),
             sidebar_present:f.text.includes('Context')});
          fs.writeFileSync(path.join(dir,'geometry-checks.json'),JSON.stringify(checks,null,2)+'\n');
          if(args['scroll-resize']==='true' && !draftCursor) throw Error('draft/cursor lost in matrix: '+name);
        }
        fs.writeFileSync(path.join(dir,'geometry-checks.json'),JSON.stringify(checks,null,2)+'\n');
        if(checks.some(c=>c.viewport_check==='FAIL')) result=1;
      }
      // This native-only clamp test changes transcript position. Run it after
      // the paired matrix, never before: both sides must enter paired captures
      // with the same repinned viewport.
      if(args['scroll-resize'] === 'true' && args.sample==='rows' && origin==='oc') {
        const markers = f => [...f.text.matchAll(/ROW-(\d+)/g)].map(m=>Number(m[1]));
        const draftBefore = await frame();
        const draftAtStart = draftBefore.cells.findIndex(line=>line.map(c=>c.symbol).join('').includes('scroll-draft'));
        if(draftAtStart<0 || !draftBefore.cursor.visible || draftBefore.cursor.y!==draftAtStart)
          throw Error('draft/cursor lost before native clamp');
        const clampChecks = [];
        const check = async (name, predicate) => {
          const f = await waitFor(predicate,name);
          const row = f.cells.findIndex(line=>line.map(c=>c.symbol).join('').includes('scroll-draft'));
          if(!f.cursor.visible || f.cursor.x!==draftBefore.cursor.x || f.cursor.y!==row)
            throw Error('draft/cursor lost: '+name);
          clampChecks.push({scenario:name,columns:f.columns,rows:f.rows,markers:markers(f),cursor:f.cursor,draft_row:row});
          fs.writeFileSync(path.join(dir,'scroll-clamp-checks.json'),JSON.stringify(clampChecks,null,2)+'\n');
          return f;
        };
        send('\x1b[<64;10;15M'.repeat(150),'scroll_to_top');
        await check('scroll-top',f=>f.text.includes('ROW-000') && !f.text.includes('ROW-089'));
        profile.columns=160; profile.rows=80;
        await page.evaluate(()=>term.resize(160,80));
        child.stdin.write(JSON.stringify({kind:'resize',columns:160,rows:80})+'\n');
        const top = await check('scroll-top-grow',f=>f.rows===80 && f.text.includes('ROW-000') && !f.text.includes('ROW-089'));
        send('\x1b[<65;10;15M','scroll_down_after_clamp');
        await check('scroll-clamped-down',f=>markers(f).at(-1)===markers(top).at(-1)+1);
        send('\x1b[<65;10;15M'.repeat(150),'scroll_repin_after_clamp');
        await check('scroll-repinned-after-clamp',f=>f.text.includes('ROW-089'));
        profile.rows=48;
        await page.evaluate(()=>term.resize(160,48));
        child.stdin.write(JSON.stringify({kind:'resize',columns:160,rows:48})+'\n');
        await waitFor(f=>f.columns===160 && f.rows===48 && f.text.includes('ROW-089'),'restore matrix geometry');
      }
      if(args.matrix === 'true') {
        send('\x1b[200~draft-one\ndraft-two\ndraft-three\x1b[201~','multiline_draft');
        const draft = await waitFor(f=>f.text.includes('draft-three') || f.text.includes('[Pasted ~3 lines]'),'multiline draft or upstream paste chip');
        await capture('multiline-draft', draft, draft.text.includes('[Pasted ~3 lines]') ? 'CAPTURED_PASTE_CHIP' : 'CAPTURED_MULTILINE_TEXT');
      }
      if(args.geometry !== 'true') {
      send('\x10','CTRL_P');
      let commandsOpened = false;
      try { await capture('commands-over-session',await waitFor(f=>f.text.includes('Commands'),'Commands',7000),'CAPTURED'); commandsOpened = true; }
      catch(e) {await capture('commands-over-session',await frame(),'FAILED_STATE'); lock.attempts.push({origin,scenario:'commands-over-session',status:'FAILED',reason:e.message}); result=1;}
      if(commandsOpened) {send('\x1b','ESCAPE'); await sleep(300);}
      send('\x18','CTRL_X'); await sleep(100); send('m','m');
      try { await capture('models-over-session',await waitFor(f=>f.text.includes('Select model'),'Select model',7000),'CAPTURED'); }
      catch(e) {await capture('models-over-session',await frame(),'FAILED_STATE'); lock.attempts.push({origin,scenario:'models-over-session',status:'FAILED',reason:e.message}); result=1;}
      if(args.variants === 'true') {
        send('\x1b','ESCAPE'); await sleep(300);
        send('/variants','SLASH_VARIANTS');
        await waitFor(f=>f.text.includes('/variants'),'variant command draft');
        send('\r','ENTER_VARIANTS');
        try { await capture('variants-over-session',await waitFor(f=>f.text.includes('Select variant') && f.text.includes('Default') && f.text.includes('none'),'Select variant',7000),'CAPTURED'); }
        catch(e) {await capture('variants-over-session',await frame(),'FAILED_STATE'); lock.attempts.push({origin,scenario:'variants-over-session',status:'FAILED',reason:e.message}); result=1;}
      }
      }
       }
       lock.attempts.push({origin,status:'EXECUTED',provider_contract:logs.filter(e=>e.kind==='provider').every(e=>e.valid)});
    } catch(e) {
      result=1; lock.attempts.push({origin,status:'FAILED',reason:e.message});
      if(tabClick && lock.tab_interactions?.[origin]) lock.tab_interactions[origin].status='FAILED';
      if(tabClose && lock.tab_close_interactions?.[origin]) lock.tab_close_interactions[origin].status='FAILED';
      if(tabCloseKey && lock.tab_close_key_interactions?.[origin]) lock.tab_close_key_interactions[origin].status='FAILED';
       if(tabRestart && lock.tab_restart_interactions?.[origin]) lock.tab_restart_interactions[origin].status='FAILED';
        if(renameSession && lock.rename_interactions?.[origin]) lock.rename_interactions[origin].status='FAILED';
        if(sidebarPalette && lock.sidebar_palette?.[origin]) lock.sidebar_palette[origin].status='FAILED';
        if(modelsInteraction && lock.models_interaction?.[origin]) lock.models_interaction[origin].status='FAILED';
        if(autocomplete && lock.autocomplete?.[origin]) lock.autocomplete[origin].status='FAILED';
        if(autocompleteKeys && lock.autocomplete_keys?.[origin]) lock.autocomplete_keys[origin].status='FAILED';
        if(ctrlC && lock.ctrl_c?.[origin]) lock.ctrl_c[origin].status='FAILED';
          if(mention && lock.mention?.[origin]) lock.mention[origin].status='FAILED';
          if(reasoningClick && lock.reasoning_click?.[origin]) lock.reasoning_click[origin].status='FAILED';
           if(reasoningSteps && lock.reasoning_steps?.[origin]) lock.reasoning_steps[origin].status='FAILED';
           if(selectionCopy && lock.selection_copy?.[origin]) lock.selection_copy[origin].status='FAILED';
            if(toastOverlap && lock.toast_overlap?.[origin]) lock.toast_overlap[origin].status='FAILED';
           if(scanner && lock.scanner?.[origin]) lock.scanner[origin].status='FAILED';
           if(twoTurn && lock.two_turn?.[origin]) lock.two_turn[origin].status='FAILED';
      await capture('failure-diagnostic',await frame(),'FAILED_STATE');
    } finally {
      if(scanner && !child.stdin.destroyed)
        child.stdin.write(JSON.stringify({kind:'release_scanner'})+'\n');
      if(!child.stdin.destroyed) child.stdin.write(JSON.stringify({kind:'stop'})+'\n');
      await closed;
      await writeQueue;
      fs.writeFileSync(path.join(dir,'raw.vt'),Buffer.concat(chunks.flat()));
       if((tabRestart || renameSession) && prequitBoundary) {
        fs.writeFileSync(path.join(dir,'restored.raw.vt'),Buffer.concat(chunks[1]));
        fs.writeFileSync(path.join(dir,'restored.protocol.json'),JSON.stringify(logs.slice(prequitBoundary.logs),null,2)+'\n');
        fs.writeFileSync(path.join(dir,'restored.inputs.json'),JSON.stringify(inputs.slice(prequitBoundary.inputs),null,2)+'\n');
        lock.generations ??= {};
        lock.generations[origin]=['prequit','restored'].map((name,g)=>({generation:g,
          root:isolated,project:path.join(isolated,'project'),home:path.join(isolated,origin,'home'),
          executable_sha256:hash,bridge_spec_sha256:sha(fs.readFileSync(path.join(dir,'bridge-spec.json'))),
          source_manifest_sha256:lock.oc.source_manifest_sha256,
          raw_vt_sha256:sha(fs.readFileSync(path.join(dir,name+'.raw.vt'))),
          protocol_sha256:sha(fs.readFileSync(path.join(dir,name+'.protocol.json'))),
          inputs_sha256:sha(fs.readFileSync(path.join(dir,name+'.inputs.json')))}));
        json('capture.lock.json',lock);
      }
      fs.writeFileSync(path.join(dir,'protocol.json'),JSON.stringify(logs,null,2)+'\n');
      fs.writeFileSync(path.join(dir,'inputs.json'),JSON.stringify(inputs,null,2)+'\n');
      fs.writeFileSync(path.join(dir,'bridge.stderr.txt'),bridgeError);
      commands.push({argv:['/usr/bin/python3',path.join(here,'bridge.py'),path.join(dir,'bridge-spec.json')],exit_code:bridgeExit});
      lock[origin].version=logs.find(e=>e.kind==='launch')?.version;
      await page.close();
    }
  }
  if(modelsInteraction) {
    const upstream=lock.models_interaction?.upstream?.checks.find(c=>c.stage==='scrolled')?.observations;
    const native=lock.models_interaction?.oc?.checks.find(c=>c.stage==='scrolled')?.observations;
    if(upstream && native) lock.models_interaction.scroll_policy_comparison={upstream,native,
      same:canonical(upstream)===canonical(native)};
  }
  lock.profile=profile;
  lock.qualification = {
    status: 'DIAGNOSTIC_BASELINES_ONLY',
    stable_state_predicate: 'Expected visible marker plus unchanged full styled grid/cursor for 5 polls, 200 ms apart; completed transcript additionally requires successful fixture response',
    requested_settings: profile.settings,
    unresolved: ['Application elapsed-time/token-rate values are real wall-clock measurements, not frozen across runs',
      ...(reasoningClick || reasoningSteps ? ['Reasoning click states are captured; whole-frame parity still depends on paired grid/PNG comparator results'] :
        ['Fixture requests 6800ms reasoning but supplies text-only output in the default sample; reasoning qualification is not executed']),
      'Location is the common isolated project path, not screenshot /tmp/space; unique attempt directory changes across reruns',
      'Rust sidebar/devtools/title/model-display differences remain visible; requested settings are not asserted as effective Rust settings',
      'Failed dialog predicates produce diagnostic actual frames, not equivalent successful dialog states'],
    mcp_error_and_stall: 'NOT_RUN (V00 three-screen capture only)'
  };
  for (const scenario of args.geometry === 'true' ? [...new Set(lock.captures.map(c=>c.scenario))]
    .filter(s=>!scanner || !s.includes('-candidate-')) : ['session-wide-completed','commands-over-session','models-over-session', ...(args.variants === 'true' ? ['variants-over-session'] : [])]) {
    for (const mode of ['grid','png']) {
      const ext=mode==='grid'?'cells.json':'png';
      const stage=scanner && scenario.startsWith('scanner-') ? scenario.slice('scanner-'.length) : null;
      const paired=stage && lock.scanner?.oc?.checks.stages[stage];
      if(stage && paired?.status==='UNMATCHED_PHASE') {
        lock.attempts.push({scenario,mode,status:'UNMATCHED_PHASE',reason:paired.reason});result=1;continue;
      }
      const ref=path.join(output,'upstream',scenario+'.'+ext);
      const actual=path.join(output,'oc',(paired?.scenario ?? scenario)+'.'+ext);
      if(paired?.phase_match==='EXACT_INDICATOR' && paired.scenario!==scenario)
        throw Error('Accepted scanner phase must have canonical scenario: '+scenario);
      if (!fs.existsSync(ref)||!fs.existsSync(actual)) {lock.attempts.push({scenario,mode,status:'BLOCKED',reason:'Missing actual capture'});result=1;continue;}
      const r=execute(['/usr/bin/python3',path.join(repo,'tui-recovery/scripts/compare_frames.py'),mode,ref,actual,'--report',path.join(output,scenario+'.'+mode+'-diff.json')]);
      lock.attempts.push({scenario,mode,status:paired?.phase_match==='UNMATCHED_PHASE' ? 'UNMATCHED_PHASE' :
        r.status===0?'EQUAL':r.status===1?'DIFFERENT':'INVALID',
        ...(paired?.phase_match==='UNMATCHED_PHASE' ? {grid_or_png_result:r.status===0?'EQUAL':r.status===1?'DIFFERENT':'INVALID'} : {}),
        ...(paired?.scenario ? {native_capture:paired.scenario,phase_match:paired.phase_match} : {}),exit_code:r.status});
      if(r.status!==0 || paired?.phase_match==='UNMATCHED_PHASE')result=1;
    }
  }
} catch(e) {lock.attempts.push({status:'BLOCKED',reason:e.stack});result=2;}
finally {
  if(browser)await browser.close();
  lock.finished=new Date().toISOString(); lock.exit_code=result;
  commands[0].exit_code=result;
  json('commands.json',commands);json('capture.lock.json',lock);
}
console.log(JSON.stringify({output,exit_code:result,attempts:lock.attempts},null,2));
process.exitCode=result;
