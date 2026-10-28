(function () {
  const { t } = window.PrimeI18n;
  const $ = (id) => document.getElementById(id);

  function create({ call, refresh }) {
    const error = $("auth-error");
    const google = $("google-start");
    const telegram = $("telegram-start");

    google.addEventListener("click", async () => {
      await call("login_google");
      refresh();
    });
    telegram.addEventListener("click", async () => {
      await call("login_telegram");
      refresh();
    });
    $("auth-cancel").addEventListener("click", async () => {
      await call("cancel_auth");
      refresh();
    });
    $("auth-reopen").addEventListener("click", () => call("open", { target: "auth_link" }));

    function render(state) {
      const waitingGoogle = state.auth === "waiting_google";
      const waitingTelegram = state.auth === "waiting_telegram";
      const waiting = waitingGoogle || waitingTelegram;

      google.disabled = waiting;
      telegram.disabled = waiting;
      google.classList.toggle("is-busy", waitingGoogle);
      telegram.classList.toggle("is-busy", waitingTelegram);
      $("auth-providers").classList.toggle("is-locked", waiting);
      $("auth-waiting").hidden = !waiting;
      $("auth-waiting-text").textContent = t(waitingGoogle ? "waitingGoogle" : "waitingTelegram");
      $("auth-reopen").hidden = !state.auth_link;

      const message = window.PrimeServerText.translate(state.auth_error);
      if (!message) {
        error.hidden = true;
      } else if (error.hidden || error.textContent !== message) {
        error.hidden = false;
        error.textContent = message;
        window.PrimeMotion.replay(error, "pv-error");
      }
    }

    return { render, moveIndicator() {} };
  }

  window.PrimeLoginView = { create };
})();
