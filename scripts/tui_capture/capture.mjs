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
if (autocomplete && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || tabClick || explorationClick || renameSession || sidebarPalette))
  throw Error('--autocomplete true requires paired binaries, --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 and no other interaction/resize modes');
if (sidebarPalette && (args.geometry !== 'true' || args.sample !== 'tools' ||
    !['hide','auto'].includes(args.sidebar) || Number(args.columns) !== 160 || Number(args.rows) !== 48 ||
    !args.reference || !args.oc || args.matrix === 'true' || args.variants === 'true' ||
    args['scroll-resize'] === 'true' || args['startup-error'] === 'true' || args['seed-root'] ||
    args.tabs === 'vertical' || args['tab-click'] === 'true' || args['exploration-click'] === 'true' ||
    args['rename-session'] === 'true'))
  throw Error('--sidebar-palette true requires paired binaries, --geometry true --sample tools --sidebar hide|auto --columns 160 --rows 48, horizontal tabs and no other interaction/resize modes');
if (regenerateTitle && !renameSession)
  throw Error('--regenerate-title true requires --rename-session true (paired Reader tools 120x40 profile)');
if (renameSession && (tabClick || tabClose || tabCloseKey || tabRestart || explorationClick ||
    args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    args['agent-profile'] !== 'true' || Number(args.columns) !== 120 || Number(args.rows) !== 40 ||
    args.matrix === 'true' || args.variants === 'true' || args['scroll-resize'] === 'true' || args['startup-error'] === 'true' ||
    args['seed-root'] || args.tabs === 'vertical' || !args.reference || !args.oc))
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
const fixtureSha = sha(canonical({files: fixtureFiles, sample: args.sample || 'table', variants: args.variants === 'true', protocol: sha(fs.readFileSync(path.join(here,'bridge.py')))}));
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
      animations: 'original supported animations=false; completed states only; terminal cursorBlink=false'}};
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
       regenerate_title: regenerateTitle};
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
    const capture = async (scenario, f, status) => {
      const name = path.join(dir,scenario);
      if (fs.existsSync(name+'.cells.json')) throw Error('Refusing to overwrite scenario: '+name);
      // Repaint the same VT buffer on both sides to distinguish a stale xterm
      // DOM row left behind by resize from a real application cell mismatch.
      // This never changes the buffer, input sequence, or comparator regions.
      const refreshed = args['refresh-before-capture'] === 'true';
      if (refreshed) await page.evaluate(() => term.refresh(0,term.rows-1));
      // The grid can settle before Chromium paints resized canvas/text layers.
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const paintWaitFrames = 2;
      const before = await page.evaluate(() => readCaptureGeometry());
      const rect = await page.locator('.xterm-screen').boundingBox();
      if (!rect) throw Error('Missing .xterm-screen screenshot bounds');
      const clip = {x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)};
      // Profile records requested shared inputs only. Measured CSS/PNG facts are
      // per-side observations and must not alter the paired environment ID.
      const environment = sha(canonical(profile));
      const {text, ...grid} = f;
      fs.writeFileSync(name+'.cells.json', JSON.stringify({schema_version:1, origin, scenario,
        fixture_sha256:fixtureSha, environment_id:environment,
        producer_commit: origin==='upstream' ? lock.sources.upstream_commit : commit, ...grid}));
      fs.writeFileSync(name+'.txt', text+'\n');
      const png = await page.screenshot({path:name+'.png', clip});
      const after = await page.evaluate(() => readCaptureGeometry());
      const render = {schema_version:1, origin, scenario, environment_id:environment,
        terminal_refresh_from_buffer:refreshed,
        paint_wait_request_animation_frames:paintWaitFrames, before, after,
        layout_changed_during_screenshot:canonical(before)!==canonical(after),
        screenshot: {clip, png_width:png.readUInt32BE(16), png_height:png.readUInt32BE(20)}};
      fs.writeFileSync(name+'.render.json',JSON.stringify(render,null,2)+'\n');
      if(sha(JSON.stringify(await frame())) !== sha(JSON.stringify(f))) {
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
      send('\x1b[200~'+fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim()+'\x1b[201~','prompt_paste');
      await sleep(200); send('\r','submit');
       const marker = ['short','reasoning','tools'].includes(args.sample) ? 'GEOMETRY-SHORT' : ['rows','rows-reflow'].includes(args.sample) ? 'ROW-089' : 'Через Code Mode';
       const done = await waitFor(f => f.text.includes(marker) &&
        /MiMo-V2.6-Flash Free · \d/.test(f.text) &&
         logs.some(e => e.kind==='provider_completed' && e.operation==='transcript'), 'completed transcript');
       if(done.text.includes('opaque-fixture-must-not-display')) throw Error('opaque reasoning leaked to the terminal');
       const completedStatus = await capture('session-wide-completed',done,'CAPTURED');
       if(autocomplete) {
         await probeAutocomplete('session');
         lock.autocomplete[origin].status='RECORDED';
         json('capture.lock.json',lock);
         lock.attempts.push({origin,status:'AUTOCOMPLETE_RECORDED',
           predicates:autocompleteChecks.map(c=>({route:c.route,stage:c.stage,...c.predicates}))});
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
      lock.attempts.push({origin,status:'EXECUTED',provider_contract:logs.filter(e=>e.kind==='provider').every(e=>e.valid)});
    } catch(e) {
      result=1; lock.attempts.push({origin,status:'FAILED',reason:e.message});
      if(tabClick && lock.tab_interactions?.[origin]) lock.tab_interactions[origin].status='FAILED';
      if(tabClose && lock.tab_close_interactions?.[origin]) lock.tab_close_interactions[origin].status='FAILED';
      if(tabCloseKey && lock.tab_close_key_interactions?.[origin]) lock.tab_close_key_interactions[origin].status='FAILED';
       if(tabRestart && lock.tab_restart_interactions?.[origin]) lock.tab_restart_interactions[origin].status='FAILED';
        if(renameSession && lock.rename_interactions?.[origin]) lock.rename_interactions[origin].status='FAILED';
        if(sidebarPalette && lock.sidebar_palette?.[origin]) lock.sidebar_palette[origin].status='FAILED';
        if(autocomplete && lock.autocomplete?.[origin]) lock.autocomplete[origin].status='FAILED';
      await capture('failure-diagnostic',await frame(),'FAILED_STATE');
    } finally {
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
  lock.profile=profile;
  lock.qualification = {
    status: 'DIAGNOSTIC_BASELINES_ONLY',
    stable_state_predicate: 'Expected visible marker plus unchanged full styled grid/cursor for 5 polls, 200 ms apart; completed transcript additionally requires successful fixture response',
    requested_settings: profile.settings,
    unresolved: ['Application elapsed-time/token-rate values are real wall-clock measurements, not frozen across runs',
      'Fixture requests 6800ms reasoning but supplies text-only output; reasoning qualification is not executed',
      'Location is the common isolated project path, not screenshot /tmp/space; unique attempt directory changes across reruns',
      'Rust sidebar/devtools/title/model-display differences remain visible; requested settings are not asserted as effective Rust settings',
      'Failed dialog predicates produce diagnostic actual frames, not equivalent successful dialog states'],
    mcp_error_and_stall: 'NOT_RUN (V00 three-screen capture only)'
  };
  for (const scenario of args.geometry === 'true' ? [...new Set(lock.captures.map(c=>c.scenario))] : ['session-wide-completed','commands-over-session','models-over-session', ...(args.variants === 'true' ? ['variants-over-session'] : [])]) {
    for (const mode of ['grid','png']) {
      const ext=mode==='grid'?'cells.json':'png';
      const ref=path.join(output,'upstream',scenario+'.'+ext), actual=path.join(output,'oc',scenario+'.'+ext);
      if (!fs.existsSync(ref)||!fs.existsSync(actual)) {lock.attempts.push({scenario,mode,status:'BLOCKED',reason:'Missing actual capture'});result=1;continue;}
      const r=execute(['/usr/bin/python3',path.join(repo,'tui-recovery/scripts/compare_frames.py'),mode,ref,actual,'--report',path.join(output,scenario+'.'+mode+'-diff.json')]);
      lock.attempts.push({scenario,mode,status:r.status===0?'EQUAL':r.status===1?'DIFFERENT':'INVALID',exit_code:r.status});
      if(r.status!==0)result=1;
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
