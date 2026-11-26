(() => {
  const trace = document.getElementById('prompt-trace');
  const map = trace.closest('.project-map');
  const form = trace.querySelector('form');
  const input = form.elements.prompt;
  const submit = form.querySelector('button');
  const status = trace.querySelector('.trace-status');
  const result = trace.querySelector('.trace-result');
  const steps = [...trace.querySelectorAll('.trace-step')];
  const panels = [...trace.querySelectorAll('[data-trace-panel]')];
  const tokens = trace.querySelector('[data-trace-tokens]');
  const choices = trace.querySelector('[data-trace-choices]');
  const decoded = trace.querySelector('[data-trace-decoded]');
  const added = trace.querySelector('[data-trace-added]');

  function show(stage) {
    trace.dataset.stage = stage;
    for (const step of steps) {
      if (step.dataset.traceStage === stage) step.setAttribute('aria-current', 'step');
      else step.removeAttribute('aria-current');
    }
    for (const panel of panels) {
      panel.hidden = panel.dataset.tracePanel !== stage;
      if (!panel.hidden) panel.querySelector('h3').focus();
    }
    map.dataset.traceNode = stage;
    map.dispatchEvent(new CustomEvent('prompt-stage', { detail: stage }));
  }

  function render({ tokens: pieces, decoded: text, choices: candidates }) {
    tokens.replaceChildren(...pieces.map(({ id, piece, text: fragment }, index) => {
      const tile = document.createElement('li');
      const raw = document.createElement('code');
      const number = document.createElement('span');
      raw.textContent = fragment.includes("\uFFFD") ? piece : fragment.replace(/ /g, "␣").replace(/\n/g, "↵").replace(/\t/g, "⇥");
      if (raw.textContent !== piece) raw.title = piece;
      number.textContent = `#${id}`;
      tile.style.setProperty('--token-delay', `${Math.min(index, 14) * 28}ms`);
      tile.append(raw, number);
      return tile;
    }));
    trace.querySelector('[data-trace-context]').textContent = pieces.map(({ id }) => id).join(' → ');
    steps[2].disabled = true;
    choices.replaceChildren(...candidates.map(({ id, piece, output }) => {
      const button = document.createElement('button');
      const raw = document.createElement('code');
      const number = document.createElement('span');
      button.className = 'brain-control trace-choice';
      button.type = 'button';
      raw.textContent = piece;
      number.textContent = `#${id}`;
      button.append(raw, number);
      button.addEventListener('click', () => {
        const hasPrefix = output.startsWith(text);
        decoded.textContent = hasPrefix ? text : output;
        added.textContent = hasPrefix ? output.slice(text.length) : '';
        trace.querySelector('[data-trace-added-id]').textContent = `#${id}`;
        steps[2].disabled = false;
        show('runtime');
      });
      return button;
    }));
    result.hidden = false;
    show('tokenizer');
  }

  trace.addEventListener('click', event => {
    const button = event.target.closest('[data-trace-stage]');
    if (button) show(button.dataset.traceStage);
  });

  form.addEventListener('submit', async event => {
    event.preventDefault();
    if (submit.disabled) return;
    const data = new URLSearchParams(new FormData(form));
    if (!input.value.trim()) {
      status.textContent = status.dataset.empty;
      input.focus();
      return;
    }
    submit.disabled = true;
    input.readOnly = true;
    result.hidden = true;
    status.textContent = status.dataset.loading;
    try {
      const response = await fetch(form.action, { method: 'POST', body: data });
      if (!response.ok) throw new Error(response.status);
      render(await response.json());
      status.textContent = '';
    } catch {
      status.textContent = status.dataset.error;
    } finally {
      submit.disabled = false;
      input.readOnly = false;
    }
  });

  trace.hidden = false;
  const jump = document.getElementById('prompt-trace-link');
  jump.hidden = false;
  jump.addEventListener('click', () => trace.querySelector('h2').focus({ preventScroll: true }));
})();
