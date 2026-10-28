(function () {
  function open(element) {
    element.hidden = false;
    element.classList.remove("is-closing");
  }

  function close(element) {
    if (element.hidden || element.classList.contains("is-closing")) return;
    element.classList.add("is-closing");
    setTimeout(() => {
      element.hidden = true;
      element.classList.remove("is-closing");
    }, 320);
  }

  window.PrimeOverlay = { open, close };
})();
