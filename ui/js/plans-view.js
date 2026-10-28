(function () {
  const { t } = window.PrimeI18n;
  const $ = (id) => document.getElementById(id);

  function money(value, digits = 0) {
    return Number(value || 0).toLocaleString(window.PrimeI18n.language === "ru" ? "ru-RU" : "en-US", {
      maximumFractionDigits: digits,
    });
  }

  function term(days) {
    if (days % 365 === 0) return window.PrimeI18n.language === "ru" ? `${days / 365} год` : `${days / 365} year`;
    if (days % 30 === 0) {
      const months = days / 30;
      if (window.PrimeI18n.language !== "ru") return `${months} month${months > 1 ? "s" : ""}`;
      return `${months} ${months === 1 ? "месяц" : months < 5 ? "месяца" : "месяцев"}`;
    }
    return t("days", { days });
  }

  function create({ call, refresh, log, onConnect }) {
    const grid = $("plans-grid");
    const checkout = window.PrimeCheckoutView.create({ call, refresh, log, onConnect });
    let renderedKey = "";
    let lastBilling = null;

    function skeleton() {
      grid.replaceChildren(
        ...Array.from({ length: 4 }, (_, index) => {
          const card = document.createElement("div");
          card.className = "pv-plan pv-plan-skeleton";
          card.style.setProperty("--d", `${index * 70}ms`);
          return card;
        }),
      );
      renderedKey = "skeleton";
    }

    function planCard(plan, index) {
      const card = document.createElement("article");
      card.className = "pv-plan";
      card.classList.toggle("is-featured", Boolean(plan.is_recommended));
      card.style.setProperty("--d", `${index * 70}ms`);

      const top = document.createElement("div");
      top.className = "pv-plan-top";
      const termLabel = document.createElement("span");
      termLabel.className = "pv-plan-term";
      termLabel.textContent = term(plan.duration_days);
      top.append(termLabel);
      if (plan.is_recommended) {
        const badge = document.createElement("span");
        badge.className = "pv-badge";
        badge.textContent = t("recommended");
        top.append(badge);
      }

      const name = document.createElement("h3");
      name.textContent = plan.name;

      const price = document.createElement("div");
      price.className = "pv-price";
      const value = document.createElement("b");
      value.textContent = money(plan.price);
      const currency = document.createElement("span");
      currency.textContent = "₽";
      price.append(value, currency);

      const perDay = document.createElement("small");
      perDay.textContent = t("perDay", { price: money(plan.price / Math.max(1, plan.duration_days), 1) });

      const button = document.createElement("button");
      button.type = "button";
      button.className = "pv-primary";
      const label = document.createElement("span");
      label.className = "pv-btn-text";
      label.textContent = t("choose");
      const arrow = document.createElement("span");
      arrow.className = "pv-btn-arrow";
      arrow.textContent = "→";
      button.append(label, arrow);
      button.addEventListener("click", () => {
        log(`plan chosen ${plan.id}`);
        checkout.open(plan, lastBilling?.account);
      });

      card.append(top, name, price, perDay, button);
      return card;
    }

    function render(state) {
      const billing = state.billing;
      lastBilling = billing;

      const account = billing.account;
      const chip = $("balance-chip");
      chip.hidden = !account;
      if (account) chip.innerHTML = `${t("balance")}: <b>${money(account.balance, 2)} ₽</b>`;

      const current = $("current-plan");
      current.hidden = !account?.plan_name;
      if (account?.plan_name) {
        $("current-plan-name").textContent = t("currentPlan", { name: account.plan_name });
        $("current-plan-left").textContent = t("daysLeft", { days: account.remaining_days ?? 0 });
      }

      const error = $("plans-error");
      error.hidden = !billing.error;
      error.textContent = window.PrimeServerText.translate(billing.error);

      if (!billing.plans.length) {
        if (renderedKey !== "skeleton" && billing.loading) skeleton();
      } else {
        const key = billing.plans.map((plan) => `${plan.id}:${plan.price}:${plan.is_recommended}`).join("|");
        if (key !== renderedKey) {
          grid.replaceChildren(...billing.plans.map(planCard));
          renderedKey = key;
        }
      }

      checkout.render(state);
    }

    function load() {
      if (!renderedKey) skeleton();
      call("billing_load");
    }

    return { render, load, renderCheckout: checkout.render };
  }

  window.PrimePlansView = { create };
})();
