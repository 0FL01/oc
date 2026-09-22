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
