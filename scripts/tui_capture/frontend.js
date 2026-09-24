// Runs in pinned Chromium. All cells come from the real xterm.js VT buffer.
window.startTerminal = ({ columns, rows }) => {
  const term = window.term = new Terminal({cols: columns, rows, allowProposedApi: true,
    fontFamily: '"DejaVu Sans Mono"', fontSize: 14, lineHeight: 1, letterSpacing: 0,
    fontWeight: 'normal', fontWeightBold: 'bold', cursorBlink: false,
    drawBoldTextInBrightColors: false, minimumContrastRatio: 1, scrollback: 0,
    theme: {foreground: '#eeeeee', background: '#0a0a0a', cursor: '#eeeeee'}});
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = '11';
  term.open(document.getElementById('terminal'));
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
  const domRows = {element_count: rows?.children.length ?? 0,
    last_rows: lastRows.map((row, index) => {
      const lastChildren = [...row.children].slice(-4);
      return {row_index: rows.children.length - lastRows.length + index, ...paint(row),
        element_child_count: row.children.length,
        last_element_children: lastChildren.map((child, childIndex) => ({
          child_index: row.children.length - lastChildren.length + childIndex, ...paint(child)}))};
    })};
  return {device_pixel_ratio: window.devicePixelRatio,
    viewport: {width: window.innerWidth, height: window.innerHeight,
      document_client_width: document.documentElement.clientWidth,
      document_client_height: document.documentElement.clientHeight},
    screen_rect: layers['.xterm-screen'].rect,
    measured_cell_width: layers['.xterm-screen'].rect.width / term.cols,
    measured_cell_height: layers['.xterm-screen'].rect.height / term.rows,
    columns: term.cols, rows: term.rows, layers, canvases, dom_rows: domRows};
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
  return {columns: term.cols, rows: term.rows, cells,
    cursor: {x: buffer.cursorX, y: buffer.cursorY, visible: !core.isCursorHidden,
      shape: core.decPrivateModes.cursorStyle || term.options.cursorStyle},
    text: cells.map(row => row.map(c => c.symbol).join('')).join('\n')};
};
