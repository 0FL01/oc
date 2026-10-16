// Runs in pinned Chromium. All cells come from the real xterm.js VT buffer.
window.startTerminal = ({ columns, rows, cursor_blink = false, cursor_renderer = 'dom', cursor_sync = 'supported' }) => {
  const term = window.term = new Terminal({cols: columns, rows, allowProposedApi: true,
    fontFamily: '"DejaVu Sans Mono"', fontSize: 14, lineHeight: 1, letterSpacing: 0,
    fontWeight: 'normal', fontWeightBold: 'bold', cursorBlink: cursor_blink,
    drawBoldTextInBrightColors: false, minimumContrastRatio: 1, scrollback: 0,
    theme: {foreground: '#eeeeee', background: '#0a0a0a', cursor: '#eeeeee'}});
  if(cursor_sync==='unsupported')for(const final of ['h','l'])term.parser.registerCsiHandler({prefix:'?',final},params=>params.length===1&&params[0]===2026);
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = '11';
  term.open(document.getElementById('terminal'));
  if(cursor_renderer==='webgl') {
    const addon=window.cursorWebgl=new WebglAddon.WebglAddon(true);
    term.loadAddon(addon);
  }
  term.focus();
  term.onData(data => window.terminalReply(data));
};
window.writeTerminal = data => new Promise(resolve => term.write(Uint8Array.from(atob(data), c => c.charCodeAt(0)), resolve));
// Geometry-only diagnostic. Read CSS/layer layout independently of the VT buffer;
// do not collect DOM text, attributes, URLs, or canvas pixels here.
window.readCaptureGeometry = () => {
  const rect = element => {
    const {x, y, width, height, right, bottom} = element.getBoundingClientRect();
    return {x, y, width, height, right, bottom};
  };
  const describe = element => {
    if (!element) return null;
    const css = getComputedStyle(element);
    return {rect: rect(element),
      layout: {client_width: element.clientWidth, client_height: element.clientHeight,
        offset_width: element.offsetWidth, offset_height: element.offsetHeight,
        scroll_width: element.scrollWidth, scroll_height: element.scrollHeight},
      css: {width: css.width, height: css.height, background_color: css.backgroundColor,
        opacity: css.opacity, position: css.position, transform: css.transform,
        overflow_x: css.overflowX, overflow_y: css.overflowY,
        padding_left: css.paddingLeft, padding_right: css.paddingRight,
        border_left_width: css.borderLeftWidth, border_right_width: css.borderRightWidth}};
  };
  const screen = document.querySelector('.xterm-screen');
  if (!screen) throw Error('Missing .xterm-screen for capture geometry');
  const layers = Object.fromEntries(['.xterm', '.xterm-viewport', '.xterm-screen',
    '.xterm-text-layer', '.xterm-rows', '.xterm-cursor-layer', '.xterm-selection-layer']
    .map(selector => [selector, describe(document.querySelector(selector))]));
  const canvases = [...screen.querySelectorAll('canvas')].map((canvas, index) => ({
    index, parent_class: canvas.parentElement?.className || '',
    intrinsic_width: canvas.width, intrinsic_height: canvas.height, ...describe(canvas)}));
  const paint = element => {
    const css = getComputedStyle(element);
    return {rect: rect(element), css: {width: css.width, position: css.position,
      color: css.color, background_color: css.backgroundColor}};
  };
  const rows = document.querySelector('.xterm-rows');
  const lastRows = rows ? [...rows.children].slice(-2) : [];
  const paintRows = rows ? [...rows.children].slice(0, 80) : [];
  const describeRow = (row, rowIndex) => {
    const buffer = term.buffer.active;
    const line = buffer.getLine(buffer.viewportY + rowIndex);
    return {row_index: rowIndex, ...paint(row),
      buffer_line_length: line?.length ?? 0,
      // Alternate-screen shrink may retain styled cells beyond the viewport.
      // Observe only bounded attribute facts, never discarded glyph content.
      outside_column_attributes: Array.from({length: Math.min(4, Math.max(0, (line?.length ?? 0) - term.cols))}, (_, index) => {
        const cell = line.getCell(term.cols + index);
        return {column: term.cols + index, fg_mode: cell.getFgColorMode(), fg: cell.getFgColor(),
          bg_mode: cell.getBgColorMode(), bg: cell.getBgColor(), bold: !!cell.isBold()};
      }),
      element_child_count: row.children.length,
      last_element_children: [...row.children].slice(-4).map((child, childIndex, children) => ({
        child_index: row.children.length - children.length + childIndex, ...paint(child)}))};
  };
  const domRows = {element_count: rows?.children.length ?? 0,
    // Bounded tail-span geometry across the viewport also diagnoses edge paint
    // on prompt/tool rows. No text, arbitrary attributes or raster is read.
    paint_rows: paintRows.map((row, index) => describeRow(row, index)),
    last_rows: lastRows.map((row, index) => describeRow(row, rows.children.length - lastRows.length + index))};
  return {device_pixel_ratio: window.devicePixelRatio,
    viewport: {width: window.innerWidth, height: window.innerHeight,
      document_client_width: document.documentElement.clientWidth,
      document_client_height: document.documentElement.clientHeight},
    screen_rect: layers['.xterm-screen'].rect,
    measured_cell_width: layers['.xterm-screen'].rect.width / term.cols,
    measured_cell_height: layers['.xterm-screen'].rect.height / term.rows,
    columns: term.cols, rows: term.rows, layers, canvases, dom_rows: domRows,
    logical_buffer_cursor: {x:term.buffer.active.cursorX,y:term.buffer.active.cursorY}};
};
window.readTerminal = () => {
  const buffer = term.buffer.active;
  const colors = term._core._themeService.colors;
  const hex = n => '#' + n.toString(16).padStart(6, '0');
  const rgb = c => hex(c.rgba >>> 8);
  const color = (cell, fg) => {
    const n = fg ? cell.getFgColor() : cell.getBgColor();
    if (fg ? cell.isFgRGB() : cell.isBgRGB()) return hex(n);
    if (fg ? cell.isFgPalette() : cell.isBgPalette()) return rgb(colors.ansi[n]);
    return rgb(fg ? colors.foreground : colors.background);
  };
  const modifiers = [['isBold', 'bold'], ['isDim', 'dim'], ['isItalic', 'italic'],
    ['isUnderline', 'underlined'], ['isBlink', 'slow_blink'], ['isInverse', 'reversed'],
    ['isInvisible', 'hidden'], ['isStrikethrough', 'crossed_out']];
  const cells = Array.from({length: term.rows}, (_, y) => Array.from({length: term.cols}, (_, x) => {
    const c = buffer.getLine(buffer.viewportY + y).getCell(x);
    return {symbol: c.getChars() || (c.getWidth() === 0 ? '' : ' '), fg: color(c, true), bg: color(c, false),
      width: c.getWidth(), modifiers: modifiers.filter(([method]) => c[method]()).map(([,name]) => name).sort()};
  }));
  const core = term._core.coreService;
  // xterm's logical cursor may be one-past-last while wrap is pending. Both
  // pinned DOM and WebGL renderers paint that cursor at cols-1. Measure their
  // physical position, retaining the raw logical value in capture geometry.
  // Do not normalize any other out-of-range state or weaken the comparator.
  if(!Number.isInteger(buffer.cursorX)||buffer.cursorX<0||buffer.cursorX>term.cols||
     !Number.isInteger(buffer.cursorY)||buffer.cursorY<0||buffer.cursorY>=term.rows)
    throw Error('Invalid logical buffer cursor');
  return {columns: term.cols, rows: term.rows, cells,
    cursor: {x: Math.min(buffer.cursorX,term.cols-1), y: buffer.cursorY, visible: !core.isCursorHidden,
      shape: core.decPrivateModes.cursorStyle || term.options.cursorStyle},
    text: cells.map(row => row.map(c => c.symbol).join('')).join('\n')};
};
