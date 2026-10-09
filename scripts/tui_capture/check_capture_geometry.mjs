#!/usr/bin/env node
// Synthetic Chromium geometry check; no application, PTY, or evidence attempt.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const tools = process.argv[2] || '/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
process.env.PLAYWRIGHT_BROWSERS_PATH = path.join(tools, 'browsers');
const require = createRequire(path.join(tools, 'package.json'));
const {chromium} = require('playwright');
const home = path.join(tools, 'capture-geometry-check-home');
fs.mkdirSync(home, {recursive:true});
const browser = await chromium.launch({headless:true, env:{HOME:home,PATH:'/usr/bin:/bin',LANG:'C.UTF-8'}});
try {
  const page = await browser.newPage({viewport:{width:1800,height:1100},deviceScaleFactor:1});
  await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
  await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
  await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
  await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
  await page.addScriptTag({path:path.join(here,'frontend.js')});
  await page.exposeFunction('terminalReply', () => {});
  await page.evaluate(() => startTerminal({columns:160,rows:48}));
  await page.evaluate(() => { document.querySelector('#terminal').dataset.private = 'geometry-secret-sentinel'; });
  await page.evaluate(() => term.resize(80,24));
  await page.evaluate(data => writeTerminal(data),Buffer.from(
    '\x1b[23;80H\x1b[48;2;30;30;30mX\x1b[24;80HX\x1b[0m\x1b[1;1H').toString('base64'));
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const geometry = await page.evaluate(() => readCaptureGeometry());
  assert.equal(geometry.device_pixel_ratio,1);
  assert.equal(geometry.viewport.width,1800);
  assert.equal(geometry.columns,80);
  assert.equal(geometry.rows,24);
  assert.deepEqual(geometry.screen_rect,geometry.layers['.xterm-screen'].rect);
  assert.equal(geometry.measured_cell_width,geometry.screen_rect.width/80);
  assert.equal(geometry.measured_cell_height,geometry.screen_rect.height/24);
  assert.ok(geometry.layers['.xterm-viewport']);
  assert.ok(geometry.layers['.xterm-screen'].css.background_color);
  assert.ok(geometry.layers['.xterm-text-layer'] || geometry.layers['.xterm-rows']);
  assert.equal(geometry.dom_rows.element_count,24);
  assert.equal(geometry.dom_rows.paint_rows.length,24);
  for (const row of geometry.dom_rows.paint_rows) {
    assert.ok(row.buffer_line_length>=80);
    assert.ok(row.outside_column_attributes.length<=4);
    for (const cell of row.outside_column_attributes) {
      assert.ok(cell.column>=80);
      assert.equal(typeof cell.bg_mode,'number');
      assert.equal(typeof cell.bg,'number');
      assert.equal(typeof cell.fg_mode,'number');
      assert.equal(typeof cell.fg,'number');
      assert.equal(typeof cell.bold,'boolean');
    }
  }
  assert.deepEqual(geometry.dom_rows.last_rows.map(row => row.row_index),[22,23]);
  for (const row of geometry.dom_rows.last_rows) {
    assert.equal(row.rect.height,16);
    assert.equal(row.rect.width,geometry.layers['.xterm-rows'].rect.width);
    assert.ok(row.last_element_children.length<=4);
    assert.deepEqual(row.last_element_children.map(child => child.child_index),
      Array.from({length:row.last_element_children.length},(_,i) => row.element_child_count-row.last_element_children.length+i));
    assert.ok(row.last_element_children.some(child => child.css.background_color==='rgb(30, 30, 30)'),
      `missing styled final-column span in row ${row.row_index}`);
    for (const element of [row,...row.last_element_children]) {
      assert.ok(element.rect && element.css.width && element.css.color && element.css.background_color);
      assert.equal(typeof element.css.position,'string');
    }
  }
  for (const canvas of geometry.canvases) {
    assert.ok(canvas.intrinsic_width>=0 && canvas.intrinsic_height>=0);
    assert.ok(canvas.rect && canvas.css.background_color);
  }
  const rect = await page.locator('.xterm-screen').boundingBox();
  const clip = {x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)};
  const png = await page.screenshot({clip});
  assert.equal(png.readUInt32BE(16),clip.width);
  assert.equal(png.readUInt32BE(20),clip.height);
  // No transcript, terminal bytes, document URL, or arbitrary DOM attributes.
  assert.equal(JSON.stringify(geometry).includes('geometry-secret-sentinel'),false);
  assert.equal(JSON.stringify(geometry).includes('X'),false);
  assert.equal(JSON.stringify(geometry).includes('http://'),false);
  // A requested clip alone is silently trimmed to the browser viewport. The
  // ordinary expanded card must retain every row beyond that viewport as well.
  for (const rows of [80,120]) {
    await page.evaluate(rows => term.resize(120,rows),rows);
    await page.evaluate(data => writeTerminal(data),Buffer.from(
      `\x1b[${rows};120HX\x1b[1;1H`).toString('base64'));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const before=await page.evaluate(() => readCaptureGeometry());
    const frame=await page.evaluate(() => readTerminal());
    const rect=await page.locator('.xterm-screen').boundingBox();
    const clip={x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)};
    assert.ok(clip.height>1100);
    const viewportOnly=await page.screenshot({clip});
    assert.equal(viewportOnly.readUInt32BE(20),1100);
    const full=await page.screenshot({clip,fullPage:true});
    assert.equal(full.readUInt32BE(16),clip.width);
    assert.equal(full.readUInt32BE(20),clip.height);
    assert.deepEqual(await page.evaluate(() => readCaptureGeometry()),before);
    assert.deepEqual(await page.evaluate(() => readTerminal()),frame);
    assert.equal(frame.cells.at(-1).at(-1).symbol,'X');
  }
  console.log('PASS: resize geometry, bounded DOM tail-row paint, CSS layers, viewport/DPR and complete tall PNG clips without grid/layout changes');
} finally { await browser.close(); }
