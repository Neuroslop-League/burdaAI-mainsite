(() => {
  const map = document.querySelector('.project-map');
  const stage = map.querySelector('.brain-viewport');
  const canvas = stage.querySelector('.map-wires');
  const context = canvas.getContext('2d');
  const nodes = new Map([...stage.querySelectorAll('[data-component]')].map(node => [node.dataset.component, node]));
  const legend = new Map([...map.querySelectorAll('.brain-legend a')].map(link => [link.dataset.mapSelect, link]));
  const panels = [...map.querySelectorAll('.map-component')];
  const edges = [...stage.querySelectorAll('[data-map-from]')].map(element => ({
    from: element.dataset.mapFrom,
    to: element.dataset.mapTo,
  }));
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const announcement = map.querySelector('.map-announcement');
  const language = document.querySelector('.language');
  let active = map.dataset.mapActive;
  const initial = active;
  let visible = false;
  let frame = 0;
  let previousPaint = 0;
  let paths = [];
  let width = 0;
  let height = 0;
  let activeColor;
  let idleColor;
  let nodeBounds = new Map();
  let brainCells = [];
  let activation;
  let cursor;

  // Outline of the left lobe in the illustration; the right is its reflection.
  const lobe = [[.47, .07], [.37, .09], [.27, .19], [.20, .29], [.15, .44],
    [.145, .58], [.18, .72], [.25, .83], [.35, .91], [.46, .94], [.483, .82],
    [.481, .51], [.478, .22]];
  const pixelColors = { lobe: '#c38dff', scan: '#64f3dd', wave: '#ddff5a', cursor: '#ff9dc5' };

  function insideLobe(x, y) {
    let inside = false;
    for (let i = 0, j = lobe.length - 1; i < lobe.length; j = i++) {
      const [a, b] = lobe[i];
      const [c, d] = lobe[j];
      if ((b > y) !== (d > y) && x < a + (c - a) * (y - b) / (d - b)) inside = !inside;
    }
    return inside;
  }

  function measureBrain() {
    const size = Math.max(5, Math.round(width / 76));
    brainCells = [];
    for (let y = size; y < height; y += size) {
      for (let x = size; x < width; x += size) {
        const left = x < width / 2;
        if (!insideLobe(left ? x / width : 1 - x / width, y / height)) continue;
        brainCells.push({ x, y, size, left, phase: ((x * 23 + y * 31) % 137) / 137 });
      }
    }
  }

  function select(id, updateHistory) {
    if (!nodes.has(id)) return;
    if (!reduced.matches) activation = { id, start: performance.now() };
    if (id === active) return;
    nodes.get(active).firstElementChild.removeAttribute('aria-current');
    nodes.get(id).firstElementChild.setAttribute('aria-current', 'true');
    legend.get(active).removeAttribute('aria-current');
    legend.get(id).setAttribute('aria-current', 'true');
    active = id;
    map.dataset.mapActive = id;
    for (const panel of panels) panel.hidden = panel.id !== `component-${id}`;
    const title = nodes.get(id).querySelector('.map-node-title').textContent;
    announcement.textContent = title;
    if (updateHistory) {
      const url = new URL(location.href);
      url.searchParams.set('node', id);
      history.pushState(null, '', url);
    }
    syncLanguageLink();
    paint(0);
  }

  function syncLanguageLink() {
    const url = new URL(language.href);
    url.searchParams.set('node', active);
    language.href = url.href;
  }

  map.addEventListener('click', event => {
    const link = event.target.closest('[data-map-select]');
    if (!link || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    const id = link.dataset.mapSelect;
    const fromPanel = link.closest('.map-component');
    select(id, true);
    if (fromPanel && fromPanel.hidden) {
      document.getElementById(`detail-${id}`).focus({ preventScroll: true });
    }
  });
  map.addEventListener('prompt-stage', ({ detail: id }) => {
    if (!reduced.matches) activation = { id, start: performance.now() };
  });
  addEventListener('popstate', () => {
    const node = new URL(location.href).searchParams.get('node');
    select(nodes.has(node) ? node : initial, false);
  });
  syncLanguageLink();

  function measure() {
    const box = canvas.getBoundingClientRect();
    if (!context) return;

    width = box.width;
    height = box.height;
    const ratio = Math.min(devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    nodeBounds = new Map([...nodes].map(([id, node]) => {
      const rect = node.getBoundingClientRect();
      return [id, {
        left: rect.left - box.left, right: rect.right - box.left,
        top: rect.top - box.top, bottom: rect.bottom - box.top,
        x: (rect.left + rect.right) / 2 - box.left,
        y: (rect.top + rect.bottom) / 2 - box.top,
      }];
    }));
    paths = edges.map((edge, index) => {
      const points = route(nodeBounds.get(edge.from), nodeBounds.get(edge.to), index);
      const segments = points.slice(1).map((end, i) => {
        const start = points[i];
        return { start, end, length: Math.hypot(end[0] - start[0], end[1] - start[1]) };
      }).filter(segment => segment.length > 0);
      return { edge, points, segments, length: segments.reduce((sum, segment) => sum + segment.length, 0) };
    });
    measureBrain();
    updateColors();
  }

  function route(from, to, index) {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const horizontal = Math.abs(dx) > Math.abs(dy) * 1.15;
    const sx = horizontal ? (dx > 0 ? from.right : from.left) : from.x;
    const sy = horizontal ? from.y : (dy > 0 ? from.bottom : from.top);
    const ex = horizontal ? (dx > 0 ? to.left - 4 : to.right + 4) : to.x;
    const ey = horizontal ? to.y : (dy > 0 ? to.top - 4 : to.bottom + 4);
    const bend = Math.min(Math.hypot(ex - sx, ey - sy) / 5, 22) * (index % 2 ? 1 : -1);
    const points = [[sx, sy]];
    for (let i = 1; i < 11; i++) {
      const t = i / 11;
      const bow = Math.sin(Math.PI * t) * bend;
      points.push([
        Math.round(sx + (ex - sx) * t + (horizontal ? 0 : bow)),
        Math.round(sy + (ey - sy) * t + (horizontal ? bow : 0)),
      ]);
    }
    points.push([ex, ey]);
    return points;
  }

  function updateColors() {
    const style = getComputedStyle(canvas);
    activeColor = style.color;
    idleColor = style.borderTopColor;
    paint(0);
  }

  function paintBrain(time) {
    const origin = activation && nodeBounds.get(activation.id);
    const progress = activation ? (time - activation.start) / 1050 : 1;
    const waveRadius = progress * width * .68;
    const window = width * .055;
    const scan = (time % 4400) / 4400;
    let color;
    for (const { x, y, size, left, phase } of brainCells) {
      const rhythm = Math.sin(time / 660 + phase * 4.5 + (left ? 0 : 1.8));
      let intensity = Math.max(0, rhythm - .4) * .21;
      let light = 'lobe';
      const scanDistance = Math.abs(y / height - (left ? scan : 1 - scan));
      if (scanDistance < .045) {
        intensity += (1 - scanDistance / .045) * (.09 + phase * .18);
        light = 'scan';
      }
      if (origin && progress >= 0 && progress < 1) {
        const distance = Math.hypot(x - origin.x, y - origin.y);
        const crest = 1 - Math.abs(distance - waveRadius) / window;
        if (crest > 0) {
          intensity = Math.max(intensity, crest * (1 - progress) * .57);
          light = 'wave';
        }
      }
      if (cursor) {
        const distance = Math.hypot(x - cursor.x, y - cursor.y) / (width * .14);
        if (distance < 1) {
          intensity += (1 - distance) * (.12 + phase * .22);
          if (light === 'lobe') light = 'cursor';
        }
      }
      if (intensity < .02) continue;
      if (light !== color) {
        context.fillStyle = pixelColors[light];
        color = light;
      }
      context.globalAlpha = intensity;
      context.fillRect(x - size / 2, y - size / 2, size - 1, size - 1);
    }
    context.globalAlpha = 1;
  }

  function drawPacket(segments, position, size) {
    for (const { start, end, length } of segments) {
      if (position <= length) {
        const progress = position / length;
        const x = start[0] + (end[0] - start[0]) * progress;
        const y = start[1] + (end[1] - start[1]) * progress;
        context.fillRect(Math.round(x) - size / 2, Math.round(y) - size / 2, size, size);
        break;
      }
      position -= length;
    }
  }

  function paint(time) {
    if (!context) return;
    context.clearRect(0, 0, width, height);
    if (time) paintBrain(time);
    // Draw inactive connections first so highlighted paths remain visible at crossings.
    for (const highlighted of [false, true]) {
      paths.forEach(({ edge, points, segments, length }, index) => {
        if ((edge.from === active || edge.to === active) !== highlighted || length === 0) return;
        context.lineWidth = highlighted ? 3 : 1.5;
        context.strokeStyle = context.fillStyle = highlighted ? activeColor : idleColor;
        context.beginPath();
        context.moveTo(...points[0]);
        for (const point of points.slice(1)) context.lineTo(...point);
        context.stroke();
        const { start, end } = segments.at(-1);
        const [x, y] = end;
        context.save();
        context.translate(x, y);
        context.rotate(Math.atan2(y - start[1], x - start[0]));
        context.fillRect(-7, -4, 2, 8);
        context.fillRect(-5, -2, 5, 4);
        context.restore();
        if (!highlighted || !time) return;

        const position = (time * 0.12 + index * 29) % length;
        for (const [offset, size, opacity] of [[0, 6, 1], [9, 4, .55], [18, 2, .32]]) {
          if (position < offset) continue;
          context.globalAlpha = opacity;
          drawPacket(segments, position - offset, size);
        }
        context.globalAlpha = 1;
      });
    }
  }

  function animate(time) {
    if (time - previousPaint >= 42) {
      paint(time);
      previousPaint = time;
    }
    frame = requestAnimationFrame(animate);
  }

  function syncMotion() {
    cancelAnimationFrame(frame);
    if (context && visible && !reduced.matches && !document.hidden) {
      frame = requestAnimationFrame(animate);
    } else {
      cursor = null;
      paint(0);
    }
  }

  stage.addEventListener('pointermove', event => {
    if (reduced.matches || event.pointerType === 'touch') return;
    const box = canvas.getBoundingClientRect();
    cursor = { x: event.clientX - box.left, y: event.clientY - box.top };
  }, { passive: true });
  stage.addEventListener('pointerleave', () => { cursor = null; });

  const observer = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    syncMotion();
  });
  observer.observe(stage);
  const resize = new ResizeObserver(measure);
  resize.observe(stage);
  for (const node of nodes.values()) resize.observe(node);
  reduced.addEventListener('change', syncMotion);
  document.addEventListener('visibilitychange', syncMotion);
  measure();
})();
