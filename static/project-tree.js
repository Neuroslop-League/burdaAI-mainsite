(() => {
  const layout = document.querySelector('.pr-tree-layout');
  const entries = [...layout.querySelectorAll('.pr-entry')];
  const inspector = layout.querySelector('.pr-inspector');
  const hint = inspector.querySelector('.pr-inspector-hint');
  const content = inspector.querySelector('.pr-inspector-content');
  const splitView = matchMedia('(min-width: 1050px)');
  let selected;

  function inspect(entry) {
    if (entry === selected) return;
    selected = entry;
    hint.hidden = Boolean(entry);
    content.replaceChildren(...(entry ? [entry.querySelector('.pr-details').cloneNode(true)] : []));
  }

  let keyboardEntry;
  for (const entry of entries) {
    entry.querySelector('summary').addEventListener('click', event => {
      keyboardEntry = event.detail === 0 && !entry.open ? entry : undefined;
    });
    entry.addEventListener('toggle', () => {
      if (entry.open) {
        for (const other of entries) if (other !== entry) other.open = false;
        inspect(entry);
        if (keyboardEntry === entry && splitView.matches) {
          content.querySelector('a').focus({ preventScroll: true });
        }
        keyboardEntry = undefined;
      } else if (selected === entry) {
        inspect(entries.find(other => other.open));
      }
    });
  }
  inspect(entries.find(entry => entry.open));
  layout.classList.add('pr-tree-enhanced');
  inspector.hidden = false;
})();
