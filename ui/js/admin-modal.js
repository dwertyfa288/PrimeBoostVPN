(function () {
  const $ = (id) => document.getElementById(id);

  function create() {
    const modal = $("admin-modal");
    document.body.appendChild(modal);

    const close = () => window.PrimeOverlay.close(modal);
    $("admin-modal-close").addEventListener("click", close);
    modal.addEventListener("click", (event) => event.target === modal && close());
    document.addEventListener("keydown", (event) => event.key === "Escape" && !modal.hidden && close());

    function show(text) {
      $("admin-modal-text").textContent = text;
      window.PrimeOverlay.open(modal);
    }

    return { show };
  }

  window.PrimeAdminModal = { create };
})();
