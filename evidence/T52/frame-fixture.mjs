// T52 diagnostic of the deterministic in-memory TestBackend frames, NOT a PTY.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
const root='/home/opencode/ai/oc';
const tools='/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
const require=createRequire(path.join(tools,'package.json'));
process.env.PLAYWRIGHT_BROWSERS_PATH=path.join(tools,'browsers');
const {chromium}=require('playwright');
const browser=await chromium.launch({headless:true,env:{HOME:path.join(tools,'frontend-check-home'),PATH:'/usr/bin:/bin',LANG:'C.UTF-8'}});
try {
  for (const phase of ['before','after']) {
    const fixture=JSON.parse(fs.readFileSync(path.join(root,`evidence/T52/frames-${phase}.json`)));
    const out=path.join(root,`evidence/T52/frames-${phase}`);
    assert.ok(!fs.existsSync(out),'immutable frame attempt already exists');
    fs.mkdirSync(out);
    for (const f of fixture.frames) {
      const page=await browser.newPage({viewport:{width:2560,height:1400},deviceScaleFactor:1});
      await page.setContent('<style>body{margin:0;background:#0a0a0a}</style><div id="terminal"></div>');
      await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
      await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
      await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
      await page.addScriptTag({path:path.join(root,'scripts/tui_capture/frontend.js')});
      await page.exposeFunction('terminalReply',()=>{});
      await page.evaluate(f=>startTerminal(f),f);
      await page.evaluate(data=>writeTerminal(data),Buffer.from(f.vt).toString('base64'));
      await page.evaluate(()=>document.fonts.ready);
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
      const rendered=await page.evaluate(()=>readTerminal());
      assert.equal(rendered.columns,f.columns);assert.equal(rendered.rows,f.rows);
      assert.equal(rendered.cursor.x,f.cursor.x);assert.equal(rendered.cursor.y,f.cursor.y);
      fs.writeFileSync(path.join(out,`${f.columns}x${f.rows}.cells.json`),JSON.stringify(rendered)+'\n');
      await page.locator('.xterm-screen').screenshot({path:path.join(out,`${f.columns}x${f.rows}.png`)});
      await page.close();
    }
  }
  const before=JSON.parse(fs.readFileSync(path.join(root,'evidence/T52/frames-before.json')));
  const after=JSON.parse(fs.readFileSync(path.join(root,'evidence/T52/frames-after.json')));
  const pairs=[];
  for (let i=0;i<before.frames.length;i++) {
    const b=before.frames[i],a=after.frames[i],size=`${b.columns}x${b.rows}`;
    assert.deepEqual(a,b,'complete TestBackend cells, modifiers, colors, cursor and VT must match');
    for (const ext of ['cells.json','png']) {
      assert.deepEqual(fs.readFileSync(path.join(root,`evidence/T52/frames-before/${size}.${ext}`)),
        fs.readFileSync(path.join(root,`evidence/T52/frames-after/${size}.${ext}`)),`${size} ${ext}`);
    }
    pairs.push({size,cells:b.columns*b.rows,raw_cells_equal:true,xterm_equal:true,png_bytes_equal:true,cursor_equal:true});
  }
  const report=path.join(root,'evidence/T52/frame-comparison.json');
  assert.ok(!fs.existsSync(report));
  fs.writeFileSync(report,JSON.stringify({kind:'T52 in-memory regression, NOT T44 parity',masks:[],pairs},null,2)+'\n');
  console.log('PASS: 14400 complete styled cells, 3 cursors, 3 byte-identical PNGs, no masks');
} finally {await browser.close();}
