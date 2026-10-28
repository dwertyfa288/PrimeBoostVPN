(function () {
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function swapText(node, text) {
    if (node.textContent === text) return;
    node.textContent = text;
    if (reduced) return;
    node.classList.remove("is-swapping");
    void node.offsetWidth;
    node.classList.add("is-swapping");
  }

  const tweens = new WeakMap();

  function tween(node, target, render, duration = 650) {
    const current = tweens.get(node);
    const from = current ? current.value : target;
    if (current) cancelAnimationFrame(current.frame);
    if (reduced || from === target) {
      tweens.set(node, { value: target, frame: 0 });
      node.textContent = render(target);
      return;
    }
    const started = performance.now();
    const state = { value: from, frame: 0 };
    tweens.set(node, state);
    const step = (now) => {
      const progress = Math.min(1, (now - started) / duration);
      const eased = 1 - Math.pow(1 - progress, 3);
      state.value = from + (target - from) * eased;
      node.textContent = render(state.value);
      if (progress < 1) state.frame = requestAnimationFrame(step);
    };
    state.frame = requestAnimationFrame(step);
  }

  function resetTween(node) {
    const current = tweens.get(node);
    if (current) cancelAnimationFrame(current.frame);
    tweens.delete(node);
  }

  function replay(node, className) {
    node.classList.remove(className);
    void node.offsetWidth;
    node.classList.add(className);
  }

  window.PrimeMotion = { swapText, tween, resetTween, replay };
})();
