function savePreference(key, value) {
  try { localStorage.setItem(key, value); } catch {}
}

const root = document.documentElement;
const systemTheme = matchMedia('(prefers-color-scheme: dark)');
const theme = document.querySelector('#theme');

function syncTheme() {
  const dark = (root.dataset.theme ?? (systemTheme.matches ? 'dark' : 'light')) === 'dark';
  theme.setAttribute('aria-checked', String(dark));
}

theme.addEventListener('click', () => {
  root.dataset.theme = theme.getAttribute('aria-checked') === 'true' ? 'light' : 'dark';
  savePreference('burda-theme', root.dataset.theme);
  syncTheme();
});
systemTheme.addEventListener('change', syncTheme);
syncTheme();
theme.hidden = false;

const filters = document.querySelector('.filters');
if (filters) {
  const filterButtons = Array.from(filters.querySelectorAll('button'));
  const cards = document.querySelectorAll('.project .card');
  for (const button of filterButtons) {
    button.addEventListener('click', () => {
      const topic = button.dataset.filter;
      for (const filter of filterButtons) filter.setAttribute('aria-pressed', String(filter === button));
      for (const card of cards) card.hidden = topic !== 'all' && card.dataset.topic !== topic;
    });
  }
  filters.hidden = false;
}

const palette = {
  ink: '#191917', night: '#21183d', dusk: '#30216d', blue: '#3328d4',
  violet: '#b8a5f9', purple: '#785fde', lime: '#d7fa3e', orange: '#ff733e',
  peach: '#ffb17e', cream: '#f5f3eb', green: '#879f29',
};

const artwork = new Map();

function registerArtwork(id, background, blocks, motion) {
  const canvas = document.getElementById(id);
  if (!canvas) return;
  const context = canvas.getContext('2d');
  if (context) artwork.set(canvas, { canvas, context, background, blocks, motion, step: -1, visible: false });
}

function paintArtwork(scene, step) {
  const { canvas, context, background, blocks, motion } = scene;
  if (background) {
    context.fillStyle = palette[background];
    context.fillRect(0, 0, canvas.width, canvas.height);
  } else {
    context.clearRect(0, 0, canvas.width, canvas.height);
  }
  for (const [color, x, y, width = 1, height = 1] of blocks.concat(motion(step))) {
    context.fillStyle = palette[color] ?? color;
    context.fillRect(x, y, width, height);
  }
  scene.step = step;
}

const skyStars = [[8, 8], [19, 17], [45, 5], [63, 13], [89, 8], [112, 19], [130, 10], [186, 15]];
const routeLights = [[83, 125], [95, 121], [111, 117], [126, 110], [123, 106], [114, 103], [107, 100], [117, 97], [127, 94]];
let scenePointer;

registerArtwork('home-effects', null, [], step => {
  const pixels = skyStars.map(([x, y], index) => {
    const bright = (step + index * 3) % 19 < 4;
    return [bright ? 'cream' : '#d8c5ef80', x, y, bright ? 2 : 1];
  });

  const meteor = step % 64;
  if (meteor >= 2 && meteor < 18) {
    const x = 51 + meteor * 3;
    const y = 4 + Math.floor(meteor / 2);
    pixels.push(['cream', x, y, 3], ['#d6b2ed80', x - 4, y - 1, 4]);
  }

  const scan = 53 + step % 20;
  pixels.push(['#d7fa3e45', 118, scan, 28], ['#9be35e30', 118, scan - 1, 28]);
  const [x, y] = routeLights[step % routeLights.length];
  pixels.push(['#fffdd6bd', x, y, 2, 2], ['#ffdd5080', 44, 78, 1, step % 8 < 4 ? 2 : 1]);

  if (scenePointer) {
    const { x, y } = scenePointer;
    pixels.push(['#fff8eaba', x - 7, y, 4], ['#fff8eaba', x + 4, y, 4],
      ['#fff8eaba', x, y - 7, 1, 4], ['#fff8eaba', x, y + 4, 1, 4]);
  }
  return pixels;
});

registerArtwork('model', 'violet', [
  ['purple', 4, 4, 7, 7], ['ink', 5, 3, 7, 7], ['cream', 6, 4, 5, 5], ['blue', 7, 5, 3, 3],
  ['ink', 12, 6, 5, 1], ['ink', 16, 6, 1, 6], ['ink', 16, 11, 6, 1],
  ['purple', 21, 8, 7, 7], ['ink', 22, 7, 7, 7], ['orange', 23, 8, 5, 5], ['lime', 24, 9, 3, 3],
  ['blue', 27, 2, 4, 3], ['cream', 2, 13, 3, 2], ['purple', 33, 11, 2, 2],
], step => {
  const route = [[12, 6], [13, 6], [14, 6], [15, 6], [16, 6], [16, 7], [16, 8],
    [16, 9], [16, 10], [16, 11], [17, 11], [18, 11], [19, 11], [20, 11], [21, 11]];
  return [['lime', ...route[step % route.length]]];
});

registerArtwork('training', 'lime', [
  ['green', 6, 4, 24, 11], ['ink', 5, 3, 24, 11], ['night', 6, 4, 22, 9],
  ['violet', 8, 10, 2, 1], ['violet', 10, 9, 3, 1], ['violet', 13, 8, 3, 1],
  ['violet', 16, 9, 3, 1], ['violet', 19, 7, 3, 1], ['violet', 22, 6, 3, 1],
  ['orange', 25, 5, 2, 2], ['blue', 32, 4, 2, 2], ['cream', 2, 11, 2, 2],
], step => {
  const chart = [10, 10, 9, 9, 9, 8, 8, 8, 9, 9, 9, 7, 7, 7, 6, 6, 6];
  const column = step % chart.length;
  return [['cream', 8 + column, chart[column]]];
});

registerArtwork('runtime', 'orange', [
  ['peach', 6, 3, 24, 13], ['ink', 5, 2, 24, 13], ['blue', 6, 3, 22, 11],
  ['violet', 6, 3, 22, 2], ['ink', 8, 3], ['orange', 10, 3], ['lime', 12, 3],
  ['cream', 9, 7, 2, 1], ['cream', 11, 8, 2, 1], ['cream', 13, 9, 2, 1],
  ['cream', 11, 10, 2, 1], ['cream', 9, 11, 2, 1], ['lime', 17, 11, 6, 1],
  ['violet', 31, 10, 3, 2], ['ink', 2, 4, 2, 2],
], step => {
  const cursor = step % 18;
  return cursor < 12
    ? [['blue', 17, 11, 6], ['lime', 17, 11, cursor % 6 + 1], ['cream', 18 + cursor % 6, 11]]
    : [];
});

const surfaces = {
  field: ['#644bd0', '#bd8dec', '#69cdbc', '#ee824b'],
  daylight: ['#9e73dc', '#e5b5f0', '#a491e8', '#f5d3ec'],
  slogan: ['#f3eb63', '#ffdd54', '#c6dc43', '#def989'],
  navigation: ['#65bce8', '#8c9fe8', '#70d8ce', '#b4c8f3'],
  heading: ['#b693ef', '#e8a4de', '#9abbea', '#d8baf5'],
  controls: ['#ff795e', '#ffc145', '#ee6ba7', '#ffad76'],
  model: ['#aa8cf2', '#f47fb3', '#ffa46d', '#72dcca'],
  training: ['#63c9f5', '#86dfb5', '#b7ee6b', '#9b9bf5'],
  runtime: ['#ffb75e', '#ee8eac', '#da985a', '#bda0ee'],
  about: ['#a8df6e', '#5cd3b5', '#f4ce65', '#83dfe3'],
};
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const field = document.querySelector('.pixel-field');
const sceneCanvas = document.querySelector('#home-effects');
const homepage = sceneCanvas !== null;
const targetSelector = '[data-pixel]';
const textures = new Map();
let playing;
let frame = 0;
let lastPaint = 0;
let surfaceTime = 0;
let hover;

const enterView = new IntersectionObserver((entries, observer) => {
  for (const { target, isIntersecting } of entries) {
    const scene = artwork.get(target);
    if (scene) {
      scene.visible = isIntersecting;
      if (isIntersecting) paintArtwork(scene, playing ? Math.floor(surfaceTime / 135) : 0);
    } else if (isIntersecting) {
      target.classList.add('pixel-entered');
      observer.unobserve(target);
    }
  }
}, { threshold: 0.12 });

for (const scene of artwork.values()) {
  paintArtwork(scene, 0);
  enterView.observe(scene.canvas);
}
for (const element of document.querySelectorAll(
  '.card, .about, .site-panel, .item-list > li, .pr-branch'
)) enterView.observe(element);

function drawTexture(state) {
  const { canvas, context, category, cells, columns, size, strength, element } = state;
  const dark = theme.getAttribute('aria-checked') === 'true';
  const colors = surfaces[!dark && category === 'field' ? 'daylight' : category];
  const intensity = strength * (dark ? 1 : 0.74) * (homepage ? 0.62 : 1);
  const local = hover?.element === element ? hover : null;
  context.clearRect(0, 0, canvas.width, canvas.height);

  for (let index = 0; index < cells.length; index++) {
    const { seed, offset, interval, opacity } = cells[index];
    const x = index % columns * size;
    const y = Math.floor(index / columns) * size;
    const progress = (surfaceTime + offset) / interval;
    const step = Math.floor(progress);
    const color = (seed + step) % colors.length;
    const blend = Math.min(1, (progress - step) * 2);
    const alpha = opacity * intensity;
    const nextAlpha = alpha * blend;
    if (blend < 1) {
      context.globalAlpha = (alpha - nextAlpha) / (1 - nextAlpha);
      context.fillStyle = colors[(color + colors.length - 1) % colors.length];
      context.fillRect(x, y, size, size);
    }
    context.globalAlpha = nextAlpha;
    context.fillStyle = colors[color];
    context.fillRect(x, y, size, size);

    if (local) {
      const distance = Math.hypot(x + size / 2 - local.x, y + size / 2 - local.y);
      const age = surfaceTime - local.start - distance * 3;
      const radius = Math.max(48, size * 6);
      if (distance < radius && age >= 0) {
        context.globalAlpha = (1 - distance / radius) * (0.12 + 0.09 * Math.sin(age / 160));
        context.fillStyle = colors[(color + 1) % colors.length];
        context.fillRect(x, y, size, size);
      }
    }
  }
}

function fitTexture(state) {
  const { element, canvas, size } = state;
  canvas.width = element.clientWidth;
  canvas.height = element.clientHeight;
  state.columns = Math.ceil(canvas.width / size);
  const count = state.columns * Math.ceil(canvas.height / size);
  while (state.cells.length < count) {
    state.cells.push({
      seed: Math.floor(Math.random() * surfaces[state.category].length),
      offset: Math.random() * 1100,
      interval: 460 + Math.random() * 640,
      opacity: 0.12 + Math.random() * 0.26,
    });
  }
  state.cells.length = count;
  drawTexture(state);
}

const visibility = new IntersectionObserver(entries => {
  for (const { target, isIntersecting } of entries) {
    const state = textures.get(target);
    state.visible = isIntersecting;
    if (isIntersecting) drawTexture(state);
  }
});
const resize = new ResizeObserver(entries => {
  for (const { target } of entries) fitTexture(textures.get(target));
});

for (const element of [field, ...document.querySelectorAll(targetSelector)]) {
  const isField = element === field;
  const canvas = isField ? field : document.createElement('canvas');
  const context = canvas.getContext('2d');
  if (!context) continue;
  if (isField) {
    document.body.classList.add('has-pixel-field');
  } else {
    canvas.className = 'pixel-texture';
    canvas.setAttribute('aria-hidden', 'true');
    element.prepend(canvas);
  }
  const key = isField ? 'field' : element.dataset.pixel;
  const compact = element.matches('button, a, h1, h2, h3, p, .foot span');
  const category = key === 'topic' ? element.closest('[data-topic]').dataset.topic : key;
  textures.set(element, {
    element,
    canvas,
    context,
    category,
    cells: [],
    columns: 0,
    size: compact ? 8 : 20,
    strength: isField ? 0.8 : compact ? 0.42 : 1,
    visible: isField,
  });
  resize.observe(element);
  if (!isField) visibility.observe(element);
}

function animate(now) {
  if (!lastPaint || now - lastPaint >= 55) {
    surfaceTime += lastPaint ? Math.min(now - lastPaint, 160) : 0;
    lastPaint = now;
    for (const state of textures.values()) if (state.visible) drawTexture(state);
    const step = Math.floor(surfaceTime / 135);
    for (const scene of artwork.values()) {
      if (scene.visible && scene.step !== step) paintArtwork(scene, step);
    }
  }
  frame = requestAnimationFrame(animate);
}

function syncMotion() {
  playing = !reducedMotion.matches;
  cancelAnimationFrame(frame);
  lastPaint = 0;
  if (playing && !document.hidden) {
    frame = requestAnimationFrame(animate);
  } else {
    hover = null;
    scenePointer = null;
    document.querySelectorAll('.pixel-burst').forEach(burst => burst.remove());
    if (!playing) for (const scene of artwork.values()) paintArtwork(scene, 0);
    for (const state of textures.values()) if (state.visible) drawTexture(state);
  }
}

reducedMotion.addEventListener('change', syncMotion);
document.addEventListener('visibilitychange', syncMotion);
for (const target of [theme, systemTheme]) {
  target.addEventListener(target === theme ? 'click' : 'change', () => {
    for (const scene of artwork.values()) paintArtwork(scene, Math.max(0, scene.step));
    for (const state of textures.values()) if (state.visible) drawTexture(state);
  });
}

document.addEventListener('pointermove', event => {
  if (!playing || event.pointerType !== 'mouse') return;
  scenePointer = null;
  if (sceneCanvas && event.target.closest('.home-scene')) {
    const rect = sceneCanvas.getBoundingClientRect();
    const scale = Math.max(rect.width / sceneCanvas.width, rect.height / sceneCanvas.height);
    scenePointer = {
      x: Math.floor((event.clientX - rect.left - rect.width / 2) / scale + sceneCanvas.width / 2),
      y: Math.floor((event.clientY - rect.top - rect.height / 2) / scale + sceneCanvas.height / 2),
    };
    hover = null;
    return;
  }
  const element = event.target.closest(targetSelector) ?? field;
  const bounds = element.getBoundingClientRect();
  const x = event.clientX - bounds.left;
  const y = event.clientY - bounds.top;
  const nearby = hover?.element === element && Math.hypot(x - hover.x, y - hover.y) < 12;
  hover = { element, x, y, start: nearby ? hover.start : surfaceTime };
}, { passive: true });
function clearPointer() {
  hover = null;
  scenePointer = null;
}
document.documentElement.addEventListener('pointerleave', clearPointer);
addEventListener('scroll', clearPointer, { passive: true });

const burstTargets = 'button:not(:disabled), .cta, .action, .language, .subnav a, .pr-trunk, .pr-node, .brain-control';
const burstDirections = [[-1, -1], [0, -1], [1, -1], [-1, 0], [1, 0], [-1, 1], [0, 1], [1, 1]];
document.addEventListener('click', event => {
  if (!playing) return;
  const control = event.target.closest(burstTargets);
  if (!control || !control.getClientRects().length) return;
  if (event.defaultPrevented && !control.matches('.brain-control[data-map-select]')) return;

  const bounds = control.getBoundingClientRect();
  const left = (event.detail === 0 ? bounds.width / 2 : event.clientX - bounds.left) - 2;
  const top = (event.detail === 0 ? bounds.height / 2 : event.clientY - bounds.top) - 2;
  const previous = control.querySelector(':scope > .pixel-burst');
  previous?.remove();

  const burst = document.createElement('span');
  burst.className = 'pixel-burst';
  burst.setAttribute('aria-hidden', 'true');
  control.append(burst);
  const animations = burstDirections.map(([x, y], index) => {
    const pixel = document.createElement('i');
    burst.append(pixel);
    const distance = index % 2 ? 19 : 26;
    const endX = Math.max(-bounds.left, Math.min(left + x * distance, root.clientWidth - bounds.left - 5));
    const endY = Math.max(-bounds.top, Math.min(top + y * distance, root.clientHeight - bounds.top - 5));
    return pixel.animate([
      { transform: `translate(${left}px, ${top}px) scale(1)`, opacity: 1 },
      { transform: `translate(${endX}px, ${endY}px) scale(.4)`, opacity: 0 },
    ], { duration: 300, easing: 'steps(6, end)' });
  });
  Promise.allSettled(animations.map(animation => animation.finished)).then(() => burst.remove());
});

syncMotion();
