// Vitral for Figma.
//
// Export: renders each selected layer three times (over black, over white and
// over nothing) and hands the PNGs to the UI, which encodes a .vitral.png
// whose blend modes keep working over any background.
// Import: places a .vitral.png back as a group of three image layers blended
// with difference, screen and multiply.
//
// This half talks to the document; ui.html (built from ui.src.html by
// build.sh) holds the WebAssembly encoder and decoder. Plain JavaScript,
// without optional chaining or object spread, which older versions of
// Figma's plugin sandbox reject.

const BLACK = [{ type: 'SOLID', color: { r: 0, g: 0, b: 0 } }];
const WHITE = [{ type: 'SOLID', color: { r: 1, g: 1, b: 1 } }];

// How a Vitral 0.1 file keeps each Figma blend mode where it meets the
// background (see BlendMode::fidelity in crates/vitral/src/blend.rs). Every
// mode is right over black and white; this is about the colours in between.
const FIDELITY = {
  PASS_THROUGH: 'exact',
  NORMAL: 'exact',
  MULTIPLY: 'exact',
  SCREEN: 'exact',
  HARD_LIGHT: 'exact',
  EXCLUSION: 'exact',
  DARKEN: 'approximate',
  LIGHTEN: 'approximate',
  DIFFERENCE: 'approximate',
  LINEAR_BURN: 'approximate',
  LINEAR_DODGE: 'approximate',
  OVERLAY: 'lost',
  SOFT_LIGHT: 'lost',
  COLOR_DODGE: 'lost',
  COLOR_BURN: 'lost',
  HUE: 'lost',
  SATURATION: 'lost',
  COLOR: 'lost',
  LUMINOSITY: 'lost',
};

// Enough to describe any real selection without stalling on huge ones.
const MAX_ANALYSED_NODES = 5000;
const MAX_NOTES = 6;
// figma.createImage refuses larger images.
const MAX_IMAGE_SIDE = 4096;

function modeLabel(mode) {
  const words = mode.toLowerCase().split('_').join(' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

// Paint and effect lists can be figma.mixed (a symbol) on text with ranges.
function listOf(node, key) {
  const value = key in node ? node[key] : null;
  return Array.isArray(value) ? value : [];
}

function hasOpaqueFill(node) {
  return listOf(node, 'fills').some(function (paint) {
    if (paint.visible === false || (paint.opacity !== undefined && paint.opacity < 1)) return false;
    if (paint.blendMode && paint.blendMode !== 'NORMAL') return false;
    if (paint.type === 'SOLID') return paint.color.a === undefined || paint.color.a === 1;
    if (paint.type === 'IMAGE' || paint.type === 'VIDEO') return true;
    return (paint.gradientStops || []).every(function (stop) {
      return stop.color.a === 1;
    });
  });
}

// Which blend modes in a layer reach the background, and how well a Vitral
// keeps them. A mode reaches the background when every container above it,
// up to the exported layer, is Pass through: a container set to Normal (or
// any other mode) is flattened on its own first, like a PNG export, and the
// modes inside it only mix with the rest of it.
function analyse(root) {
  const report = {
    reaching: 0,
    approximate: [],
    lost: [],
    isolated: [],
    backgroundBlur: [],
    filled: false,
    truncated: false,
  };
  let visited = 0;

  function note(list, name, mode) {
    const label = mode ? modeLabel(mode) : '';
    const seen = list.some(function (item) {
      return item.name === name && item.mode === label;
    });
    if (!seen && list.length < MAX_NOTES) list.push({ name: name, mode: label });
  }

  // Returns whether the node or anything inside it uses a mode other than
  // Normal or Pass through.
  function visit(node, reaches) {
    if (node.visible === false) return false;
    if (++visited > MAX_ANALYSED_NODES) {
      report.truncated = true;
      return false;
    }
    const modes = [];
    if (node.blendMode) modes.push(node.blendMode);
    listOf(node, 'fills')
      .concat(listOf(node, 'strokes'))
      .forEach(function (paint) {
        if (paint.visible !== false && paint.blendMode) modes.push(paint.blendMode);
      });
    listOf(node, 'effects').forEach(function (effect) {
      if (effect.visible === false) return;
      if (effect.blendMode) modes.push(effect.blendMode);
      if (effect.type === 'BACKGROUND_BLUR' && reaches) note(report.backgroundBlur, node.name);
    });

    let blended = false;
    modes.forEach(function (mode) {
      if (mode === 'NORMAL' || mode === 'PASS_THROUGH') return;
      blended = true;
      if (!reaches) return;
      report.reaching++;
      const fidelity = FIDELITY[mode] || 'lost';
      if (fidelity === 'approximate') note(report.approximate, node.name, mode);
      if (fidelity === 'lost') note(report.lost, node.name, mode);
    });

    // A boolean operation draws as one shape: its parts' modes don't apply.
    if ('children' in node && node.type !== 'BOOLEAN_OPERATION') {
      const passes = node.blendMode === 'PASS_THROUGH';
      let inner = false;
      node.children.forEach(function (child) {
        inner = visit(child, reaches && passes) || inner;
      });
      if (inner && reaches && !passes) note(report.isolated, node.name, node.blendMode);
      if (inner && node === root && passes && hasOpaqueFill(node)) report.filled = true;
      blended = blended || inner;
    }
    return blended;
  }

  visit(root, true);
  return report;
}

function problemWith(node) {
  if (node.type === 'SLICE') return 'Slices can’t be exported as Vitral; select the layers instead.';
  if (node.visible === false) return 'This layer is hidden.';
  if (!node.absoluteRenderBounds) return 'Nothing visible to export.';
  return null;
}

function documentColourSpace() {
  // 'LEGACY' (unmanaged, sRGB values), 'SRGB' or 'DISPLAY_P3'.
  return figma.root.documentColorProfile === 'DISPLAY_P3' ? 'display-p3' : 'srgb';
}

// Where an import goes: inside the selected frame, or on the page.
function importTarget() {
  const selection = figma.currentPage.selection;
  if (selection.length === 1 && (selection[0].type === 'FRAME' || selection[0].type === 'COMPONENT')) {
    return selection[0];
  }
  return figma.currentPage;
}

function describe() {
  const items = figma.currentPage.selection.map(function (node) {
    const problem = problemWith(node);
    const box = node.absoluteRenderBounds;
    return {
      name: node.name,
      type: node.type,
      width: box ? Math.ceil(box.x + box.width) - Math.floor(box.x) : 0,
      height: box ? Math.ceil(box.y + box.height) - Math.floor(box.y) : 0,
      problem: problem,
      report: problem ? null : analyse(node),
    };
  });
  const target = importTarget();
  figma.ui.postMessage({
    type: 'selection',
    items: items,
    colourSpace: documentColourSpace(),
    target: {
      name: target.type === 'PAGE' ? null : target.name,
      filled: target.type !== 'PAGE' && hasOpaqueFill(target),
    },
  });
}

// Renders a layer over black, over white and over nothing. A temporary frame
// holds a copy of the layer at the same place on the page, so the three
// renders share one pixel grid, and its fill is the only thing that changes.
// The copy blends with that fill as the layer blends with whatever is behind
// it, which is exactly what the Vitral has to record.
async function renderThreeTimes(node, scale) {
  const box = node.absoluteRenderBounds;
  const transform = node.absoluteTransform;
  const x0 = Math.floor(box.x);
  const y0 = Math.floor(box.y);
  const frame = figma.createFrame();
  try {
    frame.name = 'Vitral export (temporary)';
    frame.x = x0;
    frame.y = y0;
    frame.resize(Math.max(1, Math.ceil(box.x + box.width) - x0), Math.max(1, Math.ceil(box.y + box.height) - y0));
    frame.clipsContent = true;
    const copy = node.clone();
    frame.appendChild(copy);
    copy.relativeTransform = [
      [transform[0][0], transform[0][1], transform[0][2] - x0],
      [transform[1][0], transform[1][1], transform[1][2] - y0],
    ];
    // DOCUMENT keeps a Display P3 file's colours in Display P3.
    const settings = { format: 'PNG', constraint: { type: 'SCALE', value: scale }, colorProfile: 'DOCUMENT' };
    frame.fills = BLACK;
    const black = await frame.exportAsync(settings);
    frame.fills = WHITE;
    const white = await frame.exportAsync(settings);
    frame.fills = [];
    const transparent = await frame.exportAsync(settings);
    return { black: black, white: white, transparent: transparent };
  } finally {
    frame.remove();
  }
}

async function exportSelection(scale) {
  const nodes = figma.currentPage.selection.filter(function (node) {
    return !problemWith(node);
  });
  if (nodes.length === 0) {
    figma.ui.postMessage({ type: 'error', message: 'Select a visible layer or frame to export.' });
    return;
  }
  figma.ui.postMessage({ type: 'export-start', count: nodes.length });
  for (const node of nodes) {
    try {
      const renders = await renderThreeTimes(node, scale);
      figma.ui.postMessage({
        type: 'renders',
        name: node.name,
        scale: scale,
        black: renders.black,
        white: renders.white,
        transparent: renders.transparent,
      });
    } catch (e) {
      figma.ui.postMessage({ type: 'error', message: '“' + node.name + '”: ' + (e && e.message ? e.message : e) });
    }
  }
}

function centre(target, width, height) {
  if (target.type === 'PAGE') {
    const c = figma.viewport.center;
    return { x: Math.round(c.x - width / 2), y: Math.round(c.y - height / 2) };
  }
  return { x: Math.round((target.width - width) / 2), y: Math.round((target.height - height) / 2) };
}

function imageLayer(bytes, width, height, name) {
  const rect = figma.createRectangle();
  rect.name = name;
  rect.resize(width, height);
  rect.fills = [{ type: 'IMAGE', scaleMode: 'FILL', imageHash: figma.createImage(bytes).hash }];
  return rect;
}

// Places a decoded file: a Pass through group with the three layers, bottom
// to top difference, screen, multiply. Over a filled background they add up
// to the Vitral's colours; with nothing behind them they can't show its
// native look (SPEC §7, decision D9).
function place(msg) {
  if (msg.width > MAX_IMAGE_SIDE || msg.height > MAX_IMAGE_SIDE) {
    figma.ui.postMessage({ type: 'error', context: 'import', message: 'Figma takes images up to 4096 px a side.' });
    return;
  }
  const target = importTarget();
  const width = msg.width / msg.scale;
  const height = msg.height / msg.scale;
  const at = centre(target, width, height);
  let placed;
  if (msg.plain) {
    placed = imageLayer(msg.plain, width, height, msg.name);
    target.appendChild(placed);
  } else {
    const layers = [];
    const add = function (bytes, mode) {
      const rect = imageLayer(bytes, width, height, modeLabel(mode));
      rect.blendMode = mode;
      target.appendChild(rect);
      layers.push(rect);
    };
    if (msg.difference) add(msg.difference, 'DIFFERENCE');
    add(msg.screen, 'SCREEN');
    add(msg.multiply, 'MULTIPLY');
    placed = figma.group(layers, target);
    placed.name = msg.name + ' (Vitral)';
  }
  // In an auto-layout frame, keep it where it is put instead of in the flow.
  if ('layoutMode' in target && target.layoutMode !== 'NONE') placed.layoutPositioning = 'ABSOLUTE';
  placed.x = at.x;
  placed.y = at.y;
  figma.currentPage.selection = [placed];
  figma.notify(msg.plain ? 'Placed as a plain image.' : 'Placed “' + msg.name + '” as blended layers.');
}

figma.showUI(__html__, { width: 320, height: 520, themeColors: true });
figma.ui.onmessage = function (msg) {
  if (msg.type === 'ready') {
    describe();
    if (figma.command === 'import') figma.ui.postMessage({ type: 'tab', tab: 'import' });
  } else if (msg.type === 'export') {
    return exportSelection(msg.scale);
  } else if (msg.type === 'place') {
    place(msg);
  } else if (msg.type === 'notify') {
    figma.notify(msg.message, { error: !!msg.error });
  }
};
figma.on('selectionchange', describe);
figma.on('currentpagechange', describe);
