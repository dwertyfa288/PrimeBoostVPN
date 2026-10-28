(function () {
  const { t } = window.PrimeI18n;
  const $ = (id) => document.getElementById(id);

  function create({ call, refresh }) {
    const modal = $("trial-modal");
    document.body.appendChild(modal);
    const claim = $("trial-modal-claim");
    let dismissed = false;

    function dismiss() {
      dismissed = true;
      window.PrimeOverlay.close(modal);
      refresh();
    }

    $("trial-modal-later").addEventListener("click", dismiss);
    $("trial-modal-done").addEventListener("click", dismiss);
    claim.addEventListener("click", async () => {
      await call("claim_trial");
      refresh();
    });

    function render(state) {
      const signedIn = state.auth === "signed_in";
      const trial = state.trial || {};
      if (!signedIn) {
        dismissed = false;
        modal.hidden = true;
        return false;
      }
      const days = trial.days || 3;
      const offered = state.subscription === "none" && (trial.available || trial.claiming);
      const finished = trial.activated || Boolean(trial.error);
      if (dismissed || (!offered && !finished && modal.hidden)) return false;
      if (!offered && !finished) {
        window.PrimeOverlay.close(modal);
        return false;
      }

      window.PrimeOverlay.open(modal);
      modal.dataset.step = trial.activated ? "done" : trial.error ? "error" : "offer";
      $("trial-modal-days").textContent = String(days);
      $("trial-modal-title").textContent = t("trialTitle", { days });
      $("trial-modal-result").textContent = trial.activated ? t("trialActivated", { days }) : window.PrimeServerText.translate(trial.error);
      claim.disabled = Boolean(trial.claiming);
      claim.classList.toggle("is-busy", Boolean(trial.claiming));
      claim.querySelector(".pv-btn-text").textContent = t(trial.claiming ? "trialClaiming" : "trialButton");
      return true;
    }

    return { render };
  }

  window.PrimeTrialModal = { create };
})();
