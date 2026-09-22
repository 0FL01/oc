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
const output = path.resolve(args.output || path.join(repo, 'evidence/tui/recovery-v00', new Date().toISOString().replaceAll(':', '-')));
if (fs.existsSync(output)) throw Error('Refusing to overwrite attempt: ' + output);
fs.mkdirSync(output, {recursive: true});
const sha = data => createHash('sha256').update(data).digest('hex');
const canonical = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a],[b]) => a.localeCompare(b))) : v);
const json = (name, value) => fs.writeFileSync(path.join(output, name), JSON.stringify(value, null, 2) + '\n');
const fixture = path.join(repo, 'tui-recovery/fixtures');
const fixtureFiles = Object.fromEntries(fs.readdirSync(fixture).sort().map(n => [n, sha(fs.readFileSync(path.join(fixture,n)))]));
const fixtureSha = sha(canonical({files: fixtureFiles, protocol: sha(fs.readFileSync(path.join(here,'bridge.py')))}));
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
const lock = {schema_version: 1, started: new Date().toISOString(), runner_version: 1,
  runner_hashes: Object.fromEntries(['capture.mjs','frontend.js','bridge.py'].map(n => [n, sha(fs.readFileSync(path.join(here,n)))])),
  fixture_sha256: fixtureSha, fixture_files: fixtureFiles, oc: {commit, tree, dirty_diff_sha256: sha(diff)},
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
try {
  browser = await chromium.launch({headless: true, env: cleanEnv});
  const profile = {frontend: '@xterm/xterm', frontend_version: require('@xterm/xterm/package.json').version,
    vt_parser: 'xterm.js 6.0.0', chromium_version: browser.version(), playwright_version: require('playwright/package.json').version,
    font_family: 'DejaVu Sans Mono', font_match: execute(['fc-match','-f','%{family}|%{style}|%{file}|%{fontversion}', 'DejaVu Sans Mono']).stdout,
    fallback_fonts: null,
    font_size: 14, device_scale_factor: 1, dpi: 96, padding: 0, opacity: 1, ligatures: false,
    columns: 160, rows: 48, TERM: 'xterm-256color', COLORTERM: 'truecolor', locale: 'C.UTF-8',
    unicode_width_policy: '@xterm/addon-unicode11 0.9.0 (Unicode 11)',
    settings: {theme: 'opencode', mode: 'dark', sidebar: 'auto', devtools: false,
      clock_policy: 'real application wall clock; fixed provider created_at; no masking or clock claim',
      animations: 'original supported animations=false; completed states only; terminal cursorBlink=false'}};
  for (const origin of ['upstream','oc']) {
    const binary = args[origin === 'upstream' ? 'reference' : 'oc'];
    if (!binary) { lock.attempts.push({origin, status: 'SKIPPED', reason: 'No explicit binary supplied'}); continue; }
    if (!path.isAbsolute(binary)) throw Error('Binary must be an explicit absolute path');
    const hash = sha(fs.readFileSync(binary));
    if (origin === 'upstream' && hash !== '2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a') throw Error('Reference binary SHA mismatch');
    const dir = path.join(output, origin);
    fs.mkdirSync(dir);
    const spec = {binary, origin, columns: profile.columns, rows: profile.rows, isolated_root: isolated, fixture};
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
      await waitFor(f => /Build|Untitled session/.test(f.text), 'initial prompt');
      send('\x1b[200~'+fs.readFileSync(path.join(fixture,'input.txt'),'utf8').trim()+'\x1b[201~','prompt_paste');
      await sleep(200); send('\r','submit');
      const done = await waitFor(f => f.text.includes('Через Code Mode') &&
        (origin==='upstream' ? /Build · MiMo-V2.6-Flash Free · \d/.test(f.text) : /fixture\/fixture-model-1 · \d/.test(f.text)) &&
        logs.some(e => e.kind==='provider_completed' && e.operation==='transcript'), 'completed transcript');
      await capture('session-wide-completed',done,'CAPTURED');
      send('\x10','CTRL_P');
      let commandsOpened = false;
      try { await capture('commands-over-session',await waitFor(f=>f.text.includes('Commands'),'Commands',7000),'CAPTURED'); commandsOpened = true; }
      catch(e) {await capture('commands-over-session',await frame(),'FAILED_STATE'); lock.attempts.push({origin,scenario:'commands-over-session',status:'FAILED',reason:e.message}); result=1;}
      if(commandsOpened) {send('\x1b','ESCAPE'); await sleep(300);}
      send('\x18','CTRL_X'); await sleep(100); send('m','m');
      try { await capture('models-over-session',await waitFor(f=>f.text.includes('Select model'),'Select model',7000),'CAPTURED'); }
      catch(e) {await capture('models-over-session',await frame(),'FAILED_STATE'); lock.attempts.push({origin,scenario:'models-over-session',status:'FAILED',reason:e.message}); result=1;}
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
  for (const scenario of ['session-wide-completed','commands-over-session','models-over-session']) {
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
