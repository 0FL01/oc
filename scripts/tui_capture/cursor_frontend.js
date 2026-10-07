// Test-only observation of the pinned mature VT parser and actual DOM renderer.
// No command is intercepted, rewritten, delayed or emulated by this observer.
window.installCursorTrace = () => {
  const handler = term._core._inputHandler;
  const trace = window.cursorTrace = {states: [], active: null};
  const cursor = () => {
    const core=term._core.coreService, buffer=term.buffer.active;
    return {x:buffer.cursorX,y:buffer.cursorY,visible:!core.isCursorHidden,
      shape:core.decPrivateModes.cursorStyle||term.options.cursorStyle,
      synchronized:core.decPrivateModes.synchronizedOutput,
      blink:core.decPrivateModes.cursorBlink??term.options.cursorBlink};
  };
  for (const name of ['print', 'cursorPosition', 'cursorUp', 'cursorDown', 'cursorForward',
    'cursorBackward', 'cursorNextLine', 'cursorPrecedingLine', 'cursorCharAbsolute',
    'linePosAbsolute', 'hVPosition', 'restoreCursor', 'setModePrivate', 'resetModePrivate',
    'eraseInDisplay', 'eraseInLine', 'setCursorStyle', 'carriageReturn', 'lineFeed']) {
    if (typeof handler[name] !== 'function') throw Error('Missing pinned parser method '+name);
    const original = handler[name];
    handler[name] = function(...args) {
      const value = original.apply(this, args);
      if (trace.active) {
        if (trace.active.commands.length >= 50000) throw Error('Bounded cursor trace exceeded');
        trace.active.commands.push({at_ms: performance.now(), command: name,
          params: args[0]?.toArray?.() ?? null, cursor: cursor()});
      }
      return value;
    };
  }
  window.startCursorState = (name, inputOwner) => {
    if (trace.active) throw Error('Cursor state already active');
    const state = {name, input_owner: inputOwner, started_ms: performance.now(), samples: [], commands: [], snapshots: []};
    trace.active = state;
    const sample = () => {
      if (trace.active !== state) return;
      const cell = document.querySelector('.xterm-cursor');
      const css = cell && getComputedStyle(cell);
      const animation = cell?.getAnimations()[0];
      const current=cursor(), color=term._core._themeService.colors.cursor.rgba;
      const expected=[color>>>24,(color>>>16)&255,(color>>>8)&255,255];
      let raster;
      if(window.cursorWebgl) {
        const renderer=cursorWebgl._renderer, canvas=renderer._canvas;
        const gl=canvas.getContext('webgl2'), pixel=new Uint8Array(4);
        const size=renderer.dimensions.device.cell;
        // Parser coordinates can already point into an unpainted diff. Observe
        // the fixed, real input owner in the actual raster, not that parser's
        // intermediate coordinates or DEC visibility flag.
        gl.readPixels(inputOwner.x*size.width+1,canvas.height-inputOwner.y*size.height-2,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        raster={kind:'webgl',rgba:[...pixel],expected_rgba:expected,
          read_at:{x:inputOwner.x,y:inputOwner.y},
          visible:expected.every((v,i)=>v===pixel[i]),
          manager_visible:renderer._cursorBlinkStateManager.value?.isCursorVisible??true};
      } else {
        const rgb=`rgb(${expected.slice(0,3).join(', ')})`;
        raster={kind:'dom',visible:!!css&&current.visible&&
          (current.shape==='block'?css.backgroundColor===rgb:
            current.shape==='bar'?css.boxShadow!=='none':css.borderBottomStyle!=='hidden')};
      }
      state.samples.push({at_ms: performance.now(), cursor: current,raster,
        css: css ? {background: css.backgroundColor, color: css.color,
          shadow: css.boxShadow, border_bottom: css.borderBottomStyle} : null,
        animation: animation ? {current_ms: animation.currentTime,
          progress: animation.effect.getComputedTiming().progress,
          duration_ms: animation.effect.getTiming().duration} : null});
      // Read the mature renderer's real, complete, opaque terminal canvas. This
      // is not a generated image or a model-state renderer, and never repaints
      // the terminal. Collect in-browser so hover is not stalled by large RPCs.
      if(window.cursorWebgl&&state.snapshots.length<12&&
          performance.now()-(state.snapshots.at(-1)?.at_ms??-Infinity)>=450) {
        state.snapshots.push({at_ms:performance.now(),raster,
          geometry:readCaptureGeometry(),frame:readTerminal(),
          png_base64:cursorWebgl._renderer._canvas.toDataURL('image/png').split(',')[1]});
      }
      if (state.samples.length >= 1200) throw Error('Bounded raster samples exceeded');
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  };
  window.peekCursorState = () => trace.active?.samples.at(-1) ?? null;
  window.finishCursorState = () => {
    const state = trace.active;
    if (!state) throw Error('No active cursor state');
    state.finished_ms = performance.now();
    trace.states.push(state);
    trace.active = null;
    return state;
  };
};
