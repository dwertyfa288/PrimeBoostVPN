(function () {
  const { t } = window.PrimeI18n;
  const $ = (id) => document.getElementById(id);
  const BUSY_TEXT = { creating: "creating", preparing: "preparing", confirming: "confirming" };

  function money(value) {
    return Number(value || 0).toLocaleString(window.PrimeI18n.language === "ru" ? "ru-RU" : "en-US", {
      maximumFractionDigits: 2,
    });
  }

  function create({ call, refresh, log, onConnect }) {
    const modal = $("checkout");
    document.body.appendChild(modal);
    const methods = $("checkout-methods");
    const payButton = $("checkout-pay");
    let plan = null;
    let method = "sbp";
    let amount = null;
    let balance = 0;
    let quoteRequest = 0;

    function showStep(name) {
      modal.querySelectorAll(".pv-checkout-step").forEach((step) => {
        const active = step.dataset.step === name;
        if (active && !step.classList.contains("is-active")) step.classList.add("is-active");
        if (!active) step.classList.remove("is-active");
      });
    }

    function paintChoose() {
      methods.querySelectorAll("[data-method]").forEach((button) => {
        button.classList.toggle("is-active", button.dataset.method === method);
      });
      const balanceButton = methods.querySelector('[data-method="balance"]');
      const enough = amount !== null && balance >= amount;
      balanceButton.disabled = !enough;
      $("balance-hint").textContent = amount !== null && !enough
        ? t("balanceShort", { amount: money(amount - balance) })
        : t("balanceHint", { amount: money(balance) });
      if (method === "balance" && !enough) method = "sbp";
      $("checkout-amount").textContent = amount === null ? "…" : `${money(amount)} ₽`;
      payButton.disabled = amount === null;
      payButton.querySelector(".pv-btn-text").textContent = amount === null
        ? "…"
        : t(method === "balance" ? "payBalance" : "pay", { amount: money(amount) });
    }

    async function loadQuote() {
      const request = ++quoteRequest;
      amount = null;
      paintChoose();
      const result = await call("billing_quote", { plan_id: plan.id, method: method === "crypto" ? "crypto" : "sbp" });
      if (request !== quoteRequest) return;
      amount = result && typeof result.amount === "number" ? result.amount : plan.price;
      paintChoose();
    }

    methods.querySelectorAll("[data-method]").forEach((button) =>
      button.addEventListener("click", () => {
        method = button.dataset.method;
        paintChoose();
      }),
    );

    payButton.addEventListener("click", async () => {
      if (!plan) return;
      log(`checkout pay plan=${plan.id} method=${method}`);
      showStep("busy");
      $("checkout-busy-text").textContent = t("creating");
      await call("billing_buy", { plan_id: plan.id, method });
      refresh();
    });

    async function close() {
      if (modal.hidden) return;
      modal.hidden = true;
      plan = null;
      await call("billing_cancel");
      refresh();
    }

    $("checkout-close").addEventListener("click", close);
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") close();
    });
    $("checkout-open").addEventListener("click", () => call("billing_open"));
    $("checkout-open-link").addEventListener("click", () => call("billing_open"));
    $("checkout-retry").addEventListener("click", async () => {
      await call("billing_cancel");
      showStep("choose");
      loadQuote();
    });
    $("checkout-connect").addEventListener("click", async () => {
      await close();
      onConnect();
    });

    function open(selectedPlan, account) {
      plan = selectedPlan;
      balance = Number(account?.balance || 0);
      method = "sbp";
      $("checkout-plan").textContent = plan.name;
      showStep("choose");
      modal.hidden = false;
      call("billing_cancel");
      loadQuote();
    }

    function render(state) {
      if (modal.hidden || !plan) return;
      const checkout = state.billing.checkout;
      if (!checkout || checkout.plan_id !== plan.id) return;

      if (BUSY_TEXT[checkout.stage]) {
        showStep("busy");
        $("checkout-busy-text").textContent = t(BUSY_TEXT[checkout.stage]);
      } else if (checkout.stage === "ready") {
        showStep("ready");
        const image = $("checkout-qr");
        if (image.getAttribute("src") !== checkout.qr_image) image.src = checkout.qr_image;
        image.parentElement.hidden = !checkout.qr_image;
        $("checkout-ready-amount").textContent = `${money(checkout.amount)} ₽`;
        $("checkout-qr-hint").textContent = t(checkout.method === "crypto" ? "qrHintCrypto" : "qrHintSbp");
        $("checkout-qr-text").textContent = checkout.qr;
        $("checkout-qr-text").hidden = !checkout.qr || checkout.method !== "crypto";
        $("checkout-open").hidden = !checkout.has_link;
      } else if (checkout.stage === "link") {
        showStep("link");
      } else if (checkout.stage === "success") {
        showStep("success");
        $("checkout-success-title").textContent = t("successTitle", { name: checkout.plan_name });
      } else if (checkout.stage === "failed") {
        showStep("failed");
        $("checkout-error").textContent = window.PrimeServerText.translate(checkout.error);
      }
    }

    return { open, render };
  }

  window.PrimeCheckoutView = { create };
})();
