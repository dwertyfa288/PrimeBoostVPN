(function () {
  const $ = (id) => document.getElementById(id);

  function create({ onSettings }) {
    const modal = $("coverage-modal");
    document.body.appendChild(modal);
    let shown = false;

    $("coverage-close").addEventListener("click", () => window.PrimeOverlay.close(modal));
    $("coverage-settings").addEventListener("click", () => {
      window.PrimeOverlay.close(modal);
      onSettings();
    });

    function render(signedIn, blocked) {
      if (!signedIn) {
        shown = false;
        modal.hidden = true;
        return;
      }
      if (shown || blocked) return;
      shown = true;
      window.PrimeOverlay.open(modal);
    }

    return { render };
  }

  window.PrimeCoverageModal = { create };
})();
