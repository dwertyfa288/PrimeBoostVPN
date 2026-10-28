(function () {
  const { call: rawCall } = window.PrimeBackend;
  const POLL_MS = 1000;
  const $ = (id) => document.getElementById(id);

  let pollTimer = null;
  let inFlight = false;
  let page = "dash";
  let lastState = null;
  let billingRequested = false;

  async function call(method, params) {
    try {
      return await rawCall(method, params);
    } catch (error) {
      console.error(`[prime-vpn] ${method}:`, error);
      return null;
    }
  }

  function log(message) {
    call("ui_log", { message: String(message) });
  }

  let currentView = "";

  function showView(name) {
    if (name !== currentView) {
      if (currentView) log(`view ${currentView} -> ${name}`);
      currentView = name;
    }
    ["loading", "login", "dash", "settings", "plans"].forEach((view) => {
      $(`view-${view}`).hidden = view !== name;
    });
  }

  let loginView;
  let dashboardView;
  let settingsView;
  let plansView;
  let coverageModal = { render() {} };
  let trialModal = { render() { return false; } };

  function render(state) {
    if (!state || typeof state !== "object") return;
    lastState = state;
    const signedIn = state.auth === "signed_in";
    if (!signedIn) {
      page = "dash";
      billingRequested = false;
    } else if (!billingRequested) {
      billingRequested = true;
      call("billing_load");
    }

    $("account").hidden = !signedIn;
    $("account-email").textContent = state.email || "";
    $("account-avatar").textContent = (state.email || "?").charAt(0);
    const trialPending = state.subscription === "unknown" || (state.subscription === "none" && !(state.trial && state.trial.checked));
    coverageModal.render(signedIn, trialModal.render(state) || trialPending);
    $("open-settings").classList.toggle("is-current", page === "settings");
    $("open-plans").classList.toggle("is-current", page === "plans");
    $("open-home").classList.toggle("is-current", page === "dash");

    if (state.auth === "checking") {
      showView("loading");
    } else if (signedIn && page === "settings") {
      showView("settings");
      settingsView.render(state);
    } else if (signedIn && page === "plans") {
      showView("plans");
      plansView.render(state);
    } else if (signedIn) {
      showView("dash");
      dashboardView.render(state);
    } else {
      const wasHidden = $("view-login").hidden;
      showView("login");
      loginView.render(state);
      if (wasHidden) requestAnimationFrame(loginView.moveIndicator);
    }
  }

  function goTo(next) {
    page = page === next ? "dash" : next;
    if (page === "plans") plansView.load();
    render(lastState);
  }

  async function refresh() {
    if (inFlight) return;
    inFlight = true;
    try {
      render(await call("state"));
    } finally {
      inFlight = false;
    }
  }

  function startPolling() {
    clearInterval(pollTimer);
    pollTimer = setInterval(refresh, POLL_MS);
  }

  async function detectLanguage() {
    try {
      const env = window.astra && (await window.astra.getUiEnvironment());
      const code = env && (env.language || env.locale || env.lang);
      if (code) return code;
    } catch {}
    return navigator.language || "ru";
  }

  async function boot() {
    log("ui loaded");
    window.addEventListener("error", (event) => log(`ui error: ${event.message}`));
    window.addEventListener("unhandledrejection", (event) => log(`ui rejection: ${event.reason}`));
    window.PrimeTheme.watch();
    window.PrimeI18n.setLanguage(await detectLanguage());

    loginView = window.PrimeLoginView.create({ call, refresh });
    dashboardView = window.PrimeDashboardView.create({ call, refresh, openPlans: () => goTo("plans") });
    settingsView = window.PrimeSettingsView.create({ call, refresh, log });
    plansView = window.PrimePlansView.create({
      call,
      refresh,
      log,
      onConnect: async () => {
        page = "dash";
        render(lastState);
        await call("connect");
        refresh();
      },
    });

    $("open-settings").addEventListener("click", () => goTo("settings"));
    coverageModal = window.PrimeCoverageModal.create({ onSettings: () => goTo("settings") });
    trialModal = window.PrimeTrialModal.create({ call, refresh });
    $("close-settings").addEventListener("click", () => goTo("dash"));
    $("open-plans").addEventListener("click", () => goTo("plans"));
    $("open-home").addEventListener("click", () => goTo("dash"));
    $("close-plans").addEventListener("click", () => goTo("dash"));

    $("logout").addEventListener("click", async () => {
      await call("logout");
      refresh();
    });

    await refresh();
    startPolling();
    if (window.astra && typeof window.astra.onWindowFocus === "function") {
      window.astra.onWindowFocus(() => call("refresh"));
    }
  }

  boot();
})();
