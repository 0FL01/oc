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
if (explorationClick && (args.geometry !== 'true' || args.sample !== 'tools' || args.sidebar !== 'hide' ||
    Number(args.columns) !== 120 || Number(args.rows) !== 40 || args.matrix === 'true' || args['scroll-resize'] === 'true'))
  throw Error('--exploration-click true requires --geometry true --sample tools --sidebar hide --columns 120 --rows 40 without matrix/scroll-resize');
const output = path.resolve(args.output || path.join(repo, 'evidence/tui/recovery-v00', new Date().toISOString().replaceAll(':', '-')));
if (fs.existsSync(output)) throw Error('Refusing to overwrite attempt: ' + output);
fs.mkdirSync(output, {recursive: true});
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
      seed_root: args['seed-root'], session: args.session};
    fs.writeFileSync(path.join(dir,'bridge-spec.json'), JSON.stringify(spec, null, 2));
    lock[origin] = {...lock[origin], executable_path: binary, executable_sha256: hash};
    const page = await browser.newPage({viewport: {width: 1800, height: 1100}, deviceScaleFactor: 1});
    await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
    await page.addStyleTag({path: path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
    await page.addScriptTag({path: path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
    await page.addScriptTag({path: path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
    await page.addScriptTag({path: path.join(here,'frontend.js')});
    const child = spawn('/usr/bin/python3', [path.join(here,'bridge.py'), path.join(dir,'bridge-spec.json')], {env: cleanEnv, stdio: ['pipe','pipe','pipe']});
    const logs = [], chunks = [], inputs = [];
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
        chunks.push(bytes); fs.appendFileSync(path.join(dir,'raw.vt'),bytes);
        writeQueue = writeQueue.then(() => page.evaluate(d => writeTerminal(d), event.data));
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
      const rect = await page.locator('.xterm-screen').boundingBox();
      profile.pixel_width = Math.ceil(rect.width); profile.pixel_height = Math.ceil(rect.height);
      profile.measured_cell_width = rect.width/profile.columns;
      profile.measured_cell_height = rect.height/profile.rows;
      const environment = sha(canonical(profile));
      const {text, ...grid} = f;
      const name = path.join(dir,scenario);
      fs.writeFileSync(name+'.cells.json', JSON.stringify({schema_version:1, origin, scenario,
        fixture_sha256:fixtureSha, environment_id:environment,
        producer_commit: origin==='upstream' ? lock.sources.upstream_commit : commit, ...grid}));
      fs.writeFileSync(name+'.txt', text+'\n');
      await page.screenshot({path:name+'.png', clip:{x:rect.x,y:rect.y,width:profile.pixel_width,height:profile.pixel_height}});
      if(sha(JSON.stringify(await frame())) !== sha(JSON.stringify(f))) {
        status='UNSTABLE_CAPTURE'; result=1;
        lock.attempts.push({origin,scenario,status,reason:'VT grid changed during PNG capture'});
      }
      fs.writeFileSync(name+'.vt', Buffer.concat(chunks));
      lock.captures.push({origin,scenario,status,environment_id:environment,
        cells_sha256:sha(fs.readFileSync(name+'.cells.json')),png_sha256:sha(fs.readFileSync(name+'.png')),
        path:path.relative(output,name)});
      lock.profile=profile; json('capture.lock.json',lock);
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
      if(args.geometry === 'true') await capture('home', initial, 'CAPTURED');
      send('\x1b[200~'+fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim()+'\x1b[201~','prompt_paste');
      await sleep(200); send('\r','submit');
       const marker = ['short','reasoning','tools'].includes(args.sample) ? 'GEOMETRY-SHORT' : args.sample === 'rows' ? 'ROW-089' : 'Через Code Mode';
       const done = await waitFor(f => f.text.includes(marker) &&
        /MiMo-V2.6-Flash Free · \d/.test(f.text) &&
         logs.some(e => e.kind==='provider_completed' && e.operation==='transcript'), 'completed transcript');
       if(done.text.includes('opaque-fixture-must-not-display')) throw Error('opaque reasoning leaked to the terminal');
      await capture('session-wide-completed',done,'CAPTURED');
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
        const check = async (name, predicate) => {
          const f = await waitFor(predicate, name);
          await capture(name, f, 'CAPTURED_SCROLL_GEOMETRY');
          const draftRow = f.cells.findIndex(row=>row.map(c=>c.symbol).join('').includes('scroll-draft'));
          const rowText = draftRow < 0 ? '' : f.cells[draftRow].map(c=>c.symbol).join('');
          const draftX = rowText.indexOf('scroll-draft');
          const c = {scenario:name, markers:markers(f), cursor:f.cursor, draft_row:draftRow,
            cursor_at_draft_end:f.cursor.visible && f.cursor.x===draftX+12 && f.cursor.y===draftRow};
          checks.push(c);
          fs.writeFileSync(path.join(dir,'scroll-checks.json'),JSON.stringify(checks,null,2)+'\n');
          if(!c.cursor_at_draft_end) throw Error('draft/cursor lost: '+name);
          return c;
        };
        send('scroll-draft','single_line_draft');
        await check('scroll-pinned-draft',f=>f.text.includes('scroll-draft') && f.text.includes('ROW-089'));
        send((origin==='oc' ? '\x1b[A' : '\x1b\x19').repeat(12),'scroll_away_12_lines');
        const away = await check('scroll-away',f=>f.text.includes('scroll-draft') && markers(f).length>0 && !f.text.includes('ROW-089'));
        for(const [columns,rows,name] of [[80,24,'scroll-shrink'],[160,48,'scroll-grow']]) {
          profile.columns=columns; profile.rows=rows;
          await page.evaluate(({columns,rows})=>term.resize(columns,rows),{columns,rows});
          child.stdin.write(JSON.stringify({kind:'resize',columns,rows})+'\n');
          const c = await check(name,f=>f.columns===columns && f.rows===rows && f.text.includes('scroll-draft') && !f.text.includes('ROW-089'));
          if(origin==='oc' && c.markers.at(-1)!==away.markers.at(-1)) throw Error('native scroll offset lost during resize');
        }
        send(origin==='oc' ? '\x1b[B' : '\x1b\x05','scroll_down_one');
        const down = await check('scroll-down-one',f=>markers(f).at(-1)===away.markers.at(-1)+1);
        send((origin==='oc' ? '\x1b[B' : '\x1b\x05').repeat(100),'scroll_repin');
        await check('scroll-repinned',f=>f.text.includes('ROW-089') && f.text.includes('scroll-draft'));
        if(origin==='oc') {
          send('\x1b[A'.repeat(150),'scroll_to_top');
          await check('scroll-top',f=>f.text.includes('ROW-000') && !f.text.includes('ROW-089'));
          profile.rows=80;
          await page.evaluate(()=>term.resize(160,80));
          child.stdin.write(JSON.stringify({kind:'resize',columns:160,rows:80})+'\n');
          const top = await check('scroll-top-grow',f=>f.rows===80 && f.text.includes('ROW-000') && !f.text.includes('ROW-089'));
          send('\x1b[B','scroll_down_after_clamp');
          await check('scroll-clamped-down',f=>markers(f).at(-1)===top.markers.at(-1)+1);
        }
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
          checks.push({scenario:name, columns, rows, visible_row_markers:count,
            viewport_check:args.sample==='rows' && rows>=40 ? (count>20?'PASS':'FAIL') : 'NOT_APPLICABLE',
            sidebar_present:f.text.includes('Context')});
          fs.writeFileSync(path.join(dir,'geometry-checks.json'),JSON.stringify(checks,null,2)+'\n');
        }
        send('\x1b[200~draft-one\ndraft-two\ndraft-three\x1b[201~','multiline_draft');
        const draft = await waitFor(f=>f.text.includes('draft-three') || f.text.includes('[Pasted ~3 lines]'),'multiline draft or upstream paste chip');
        await capture('multiline-draft', draft, draft.text.includes('[Pasted ~3 lines]') ? 'CAPTURED_PASTE_CHIP' : 'CAPTURED_MULTILINE_TEXT');
        fs.writeFileSync(path.join(dir,'geometry-checks.json'),JSON.stringify(checks,null,2)+'\n');
        if(checks.some(c=>c.viewport_check==='FAIL')) result=1;
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
      await capture('failure-diagnostic',await frame(),'FAILED_STATE');
    } finally {
      if(!child.stdin.destroyed) child.stdin.write(JSON.stringify({kind:'stop'})+'\n');
      await closed;
      await writeQueue;
      fs.writeFileSync(path.join(dir,'raw.vt'),Buffer.concat(chunks));
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
