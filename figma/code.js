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

function errorText(e) {
  return e && e.message ? e.message : String(e);
}

// Where an import goes: inside the selected frame, or on the page.
function importTarget() {
  const selection = figma.currentPage.selection;
  if (selection.length === 1 && (selection[0].type === 'FRAME' || selection[0].type === 'COMPONENT')) {
    return selection[0];
  }
  return figma.currentPage;
}

// --- Vitral files already in the document as plain images ------------------

// A PNG carries Vitral data in a `viTr` chunk (SPEC §4). Checking for the
// chunk is enough to offer a rebuild; the UI decodes the file properly.
function hasVitralChunk(bytes) {
  let i = 8;
  while (i + 8 <= bytes.length) {
    const length = ((bytes[i] << 24) | (bytes[i + 1] << 16) | (bytes[i + 2] << 8) | bytes[i + 3]) >>> 0;
    const type = String.fromCharCode(bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]);
    if (type === 'viTr') return true;
    if (type === 'IEND') return false;
    i += 12 + length;
  }
  return false;
}

function imageFillOf(node) {
  const fills = listOf(node, 'fills').filter(function (paint) {
    return paint.visible !== false;
  });
  return fills.length === 1 && fills[0].type === 'IMAGE' && fills[0].imageHash ? fills[0] : null;
}

// Selected image layers whose image is a Vitral file, for instance one dragged
// onto the canvas while the plugin was closed, which Figma placed as a plain
// image. Found only if Figma kept the file's bytes as they were.
const candidates = {};
let candidateCheck = 0;

async function findCandidates() {
  const check = ++candidateCheck;
  const found = [];
  const nodes = figma.currentPage.selection.slice(0, 8);
  for (const node of nodes) {
    const fill = imageFillOf(node);
    if (!fill) continue;
    try {
      const image = figma.getImageByHash(fill.imageHash);
      const bytes = image ? await image.getBytesAsync() : null;
      if (bytes && hasVitralChunk(bytes)) found.push({ node: node, bytes: bytes });
    } catch (e) {
      // An image that can't be read is simply not offered.
    }
  }
  if (check !== candidateCheck) return;
  Object.keys(candidates).forEach(function (key) {
    delete candidates[key];
  });
  figma.ui.postMessage({
    type: 'candidates',
    items: found.map(function (c) {
      candidates[c.node.id] = c;
      return { key: c.node.id, name: c.node.name };
    }),
  });
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
  findCandidates();
}

// --- export -----------------------------------------------------------------

// Position of a node in the layer tree, for stacking copies bottom to top.
function stackPath(node) {
  const path = [];
  while (node.parent) {
    path.unshift(node.parent.children.indexOf(node));
    node = node.parent;
  }
  return path;
}

function compareStack(a, b) {
  const pa = stackPath(a);
  const pb = stackPath(b);
  for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
    if (pa[i] !== pb[i]) return pa[i] - pb[i];
  }
  return pa.length - pb.length;
}

// Renders layers over black, over white and over nothing. A temporary frame
// holds copies of the layers with their place on the page, so the three
// renders share one pixel grid, and its fill is the only thing that changes.
// The copies blend with that fill as the layers blend with whatever is behind
// them, which is exactly what the Vitral has to record.
//
// The frame sits beyond the right edge of the visible canvas, so nothing
// flickers while the fill changes. It is moved by whole pixels only, which
// keeps the artwork's alignment to the pixel grid.
async function renderThreeTimes(nodes, scale) {
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  const placed = nodes.map(function (node) {
    const box = node.absoluteRenderBounds;
    left = Math.min(left, box.x);
    top = Math.min(top, box.y);
    right = Math.max(right, box.x + box.width);
    bottom = Math.max(bottom, box.y + box.height);
    return { node: node, transform: node.absoluteTransform };
  });
  const x0 = Math.floor(left);
  const y0 = Math.floor(top);
  const view = figma.viewport.bounds;
  const frame = figma.createFrame();
  try {
    frame.name = 'Vitral export (temporary)';
    frame.fills = [];
    frame.x = Math.ceil(view.x + view.width) + 1000;
    frame.y = Math.floor(view.y);
    frame.resize(Math.max(1, Math.ceil(right) - x0), Math.max(1, Math.ceil(bottom) - y0));
    frame.clipsContent = true;
    placed.forEach(function (p) {
      const copy = p.node.clone();
      frame.appendChild(copy);
      copy.relativeTransform = [
        [p.transform[0][0], p.transform[0][1], p.transform[0][2] - x0],
        [p.transform[1][0], p.transform[1][1], p.transform[1][2] - y0],
      ];
    });
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

function togetherName(nodes) {
  const parent = nodes[0].parent;
  const shared = nodes.every(function (n) {
    return n.parent === parent;
  });
  return shared && parent.type !== 'PAGE' ? parent.name : 'Selection';
}

async function exportSelection(scale, together) {
  const nodes = figma.currentPage.selection
    .filter(function (node) {
      return !problemWith(node);
    })
    .sort(compareStack);
  if (nodes.length === 0) {
    figma.ui.postMessage({ type: 'error', message: 'Select a visible layer or frame to export.' });
    return;
  }
  const groups = together && nodes.length > 1 ? [nodes] : nodes.map(function (n) {
    return [n];
  });
  figma.ui.postMessage({ type: 'export-start', count: groups.length });
  for (const group of groups) {
    const name = group.length > 1 ? togetherName(group) : group[0].name;
    try {
      const renders = await renderThreeTimes(group, scale);
      figma.ui.postMessage({
        type: 'renders',
        name: name,
        scale: scale,
        black: renders.black,
        white: renders.white,
        transparent: renders.transparent,
      });
    } catch (e) {
      figma.ui.postMessage({ type: 'error', message: '“' + name + '”: ' + errorText(e) });
    }
  }
}

// --- import -----------------------------------------------------------------

// Files waiting for the UI to decode them, and where each one goes.
const placements = {};
let placementId = 0;

function decodeFor(bytes, name, scale, placement) {
  const id = ++placementId;
  placements[id] = placement;
  figma.ui.postMessage({ type: 'decode', id: id, bytes: bytes, name: name, scale: scale });
}

function scaleFromName(name) {
  const match = /@(\d)x/i.exec(name);
  return match && Number(match[1]) >= 1 && Number(match[1]) <= 4 ? Number(match[1]) : 1;
}

function baseName(name) {
  return name.replace(/(@\dx)?(\.vitral)?\.png$/i, '') || 'Vitral';
}

function imageLayer(bytes, width, height, name) {
  const rect = figma.createRectangle();
  rect.name = name;
  rect.resize(width, height);
  rect.fills = [{ type: 'IMAGE', scaleMode: 'FILL', imageHash: figma.createImage(bytes).hash }];
  return rect;
}

// Places a decoded file: a Pass through group with the layers, bottom to top
// difference (when the file needs it), screen, multiply. Over a filled
// background they add up to the Vitral's colours; with nothing behind them
// they can't show its native look (SPEC §7, decision D9).
//
// Where it goes: `replace` puts it exactly where an image layer was and
// removes that layer; `parent` with `centre` puts it centred on a point of a
// frame or the page; otherwise it goes in the middle of the selected frame,
// or of the screen.
function place(msg) {
  const placement = placements[msg.id] || {};
  delete placements[msg.id];
  if (msg.width > MAX_IMAGE_SIDE || msg.height > MAX_IMAGE_SIDE) {
    figma.ui.postMessage({ type: 'error', context: 'import', message: 'Figma takes images up to 4096 px a side.' });
    return;
  }
  const replaced = placement.replace;
  const parent = replaced ? replaced.parent : placement.parent || importTarget();
  const width = replaced ? replaced.width : msg.width / msg.scale;
  const height = replaced ? replaced.height : msg.height / msg.scale;
  const layers = [];
  const add = function (bytes, mode, name) {
    const rect = imageLayer(bytes, width, height, name);
    rect.blendMode = mode;
    parent.appendChild(rect);
    if (replaced) rect.relativeTransform = replaced.relativeTransform;
    layers.push(rect);
  };
  let placed;
  if (msg.plain) {
    add(msg.plain, 'NORMAL', msg.name);
    placed = layers[0];
  } else {
    if (msg.difference) add(msg.difference, 'DIFFERENCE', 'Difference');
    add(msg.screen, 'SCREEN', 'Screen');
    add(msg.multiply, 'MULTIPLY', 'Multiply');
    placed = figma.group(layers, parent);
    placed.name = msg.name + ' (Vitral)';
  }
  if (replaced) {
    parent.insertChild(parent.children.indexOf(replaced), placed);
    replaced.remove();
  } else {
    // In an auto-layout frame, keep it where it is put instead of in the flow.
    if ('layoutMode' in parent && parent.layoutMode !== 'NONE') placed.layoutPositioning = 'ABSOLUTE';
    let x;
    let y;
    if (placement.centre) {
      x = placement.centre.x - width / 2;
      y = placement.centre.y - height / 2;
    } else if (parent.type === 'PAGE') {
      x = figma.viewport.center.x - width / 2;
      y = figma.viewport.center.y - height / 2;
    } else {
      x = (parent.width - width) / 2;
      y = (parent.height - height) / 2;
    }
    placed.x = Math.round(x);
    placed.y = Math.round(y);
  }
  figma.currentPage.selection = [placed];
  figma.notify(msg.plain ? 'Placed as a plain image.' : 'Placed “' + msg.name + '” as blended layers.');
}

// While the plugin is open, a .vitral.png dragged onto the canvas is rebuilt
// as blended layers where it lands, instead of being placed as a flat image.
function onDrop(event) {
  const files = (event.files || []).filter(function (file) {
    return /\.vitral\.png$/i.test(file.name);
  });
  if (files.length === 0) return true;
  const parent = event.node && 'appendChild' in event.node ? event.node : figma.currentPage;
  files.forEach(function (file, i) {
    file.getBytesAsync().then(
      function (bytes) {
        decodeFor(bytes, baseName(file.name), scaleFromName(file.name), {
          parent: parent,
          centre: { x: event.x + i * 24, y: event.y + i * 24 },
        });
      },
      function (e) {
        figma.notify('Couldn’t read ' + file.name + ': ' + errorText(e), { error: true });
      },
    );
  });
  return false;
}

function rebuild(key) {
  const candidate = candidates[key];
  if (!candidate || candidate.node.removed) return;
  decodeFor(candidate.bytes, candidate.node.name.replace(/(\.vitral)?\.png$/i, ''), 1, { replace: candidate.node });
}

figma.showUI(__html__, { width: 320, height: 560, themeColors: true });
figma.ui.onmessage = function (msg) {
  if (msg.type === 'ready') {
    describe();
  } else if (msg.type === 'export') {
    return exportSelection(msg.scale, msg.together);
  } else if (msg.type === 'place') {
    place(msg);
  } else if (msg.type === 'rebuild') {
    rebuild(msg.key);
  } else if (msg.type === 'notify') {
    figma.notify(msg.message, { error: !!msg.error });
  }
};
figma.on('selectionchange', describe);
figma.on('currentpagechange', describe);
figma.on('drop', onDrop);
