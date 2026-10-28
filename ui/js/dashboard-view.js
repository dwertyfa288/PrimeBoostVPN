(function () {
  const { t } = window.PrimeI18n;
  const format = window.PrimeFormat;
  const motion = window.PrimeMotion;
  const $ = (id) => document.getElementById(id);

  function locationName(location) {
    if (!location) return "";
    return window.PrimeI18n.language === "ru" ? location.name : location.name_en;
  }

  function create({ call, refresh, openPlans }) {
    const orb = $("orb");
    const power = $("power");
    const routing = $("routing");
    const routingButtons = routing.querySelectorAll("[data-routing]");
    const stats = $("stats");
    const sparkline = window.PrimeSparkline.create($("spark-line"), $("spark-area"));
    let lastState = null;
    let lastUptime = -1;

    power.addEventListener("click", async () => {
      if (!lastState) return;
      const command = lastState.vpn === "disconnected" ? "connect" : "disconnect";
      await call(command);
      refresh();
    });

    routingButtons.forEach((button) =>
      button.addEventListener("click", async () => {
        if (!lastState) return;
        routing.dataset.active = button.dataset.routing;
        await call("set_settings", { ...lastState.settings, routing: button.dataset.routing });
        refresh();
      }),
    );

    $("subscribe").addEventListener("click", openPlans);
    $("trial-claim").addEventListener("click", async () => {
      await call("claim_trial");
      refresh();
    });
    $("sub-chip").addEventListener("click", openPlans);

    function renderStatus(state) {
      const on = state.vpn === "connected";
      const busy = state.vpn === "connecting" || state.vpn === "disconnecting";
      const key = on ? "on" : busy ? "busy" : "off";

      $("status-dot").classList.toggle("is-ok", on);
      motion.swapText($("status-eyebrow"), t(on ? "eyebrowOn" : busy ? "eyebrowBusy" : "eyebrowOff"));
      motion.swapText($("status-line1"), t(`${key}Line1`));
      motion.swapText($("status-line2"), t(`${key}Line2`));

      const ping = $("ping");
      const latency = on && state.location ? state.location.latency_ms : null;
      ping.hidden = latency == null;
      if (latency != null) $("ping-value").textContent = `${latency} ms`;

      motion.swapText(
        $("status-detail"),
        on && state.location
          ? t("detailOn", { location: locationName(state.location) })
          : t(busy ? "detailBusy" : "detailOff"),
      );
    }

    function renderPower(state) {
      const previous = orb.dataset.state;
      orb.dataset.state = state.vpn;
      if (previous === "connecting" && state.vpn === "connected") {
        motion.replay($("orb-burst"), "is-firing");
      }
      power.disabled = state.vpn === "disconnecting";
      const labels = {
        connected: "powerDisconnect",
        connecting: "powerConnecting",
        disconnecting: "powerDisconnecting",
        disconnected: "powerConnect",
      };
      motion.swapText($("power-label"), t(labels[state.vpn] || "powerConnect"));
      motion.swapText(
        $("stage"),
        state.vpn === "connected" ? t("proxyNote", { port: state.proxy_port }) : window.PrimeServerText.translate(state.stage),
      );
    }

    function renderStats(state) {
      const on = state.vpn === "connected";
      stats.classList.toggle("is-live", on);
      $("stat-time").textContent = format.duration(on ? state.uptime_secs : 0);

      const traffic = state.traffic;
      if (!on) {
        ["stat-down", "stat-up", "stat-total"].forEach((id) => {
          motion.resetTween($(id));
          $(id).textContent = "—";
        });
        sparkline.clear();
        lastUptime = -1;
        return;
      }
      motion.tween($("stat-down"), traffic.down_speed, format.speed);
      motion.tween($("stat-up"), traffic.up_speed, format.speed);
      motion.tween($("stat-total"), traffic.down_total + traffic.up_total, format.size);
      if (state.uptime_secs !== lastUptime) {
        sparkline.push(traffic.down_speed);
        lastUptime = state.uptime_secs;
      }
    }

    function render(state) {
      lastState = state;
      document.body.dataset.pvOnline = String(state.vpn === "connected");
      renderStatus(state);
      renderPower(state);
      renderStats(state);

      routing.dataset.active = state.settings.routing;
      routingButtons.forEach((button) =>
        button.classList.toggle("is-active", button.dataset.routing === state.settings.routing),
      );

      const trial = state.trial || {};
      const trialOffered = state.subscription === "none" && (trial.available || trial.claiming);
      $("trial-banner").hidden = !trialOffered;
      $("trial-title").textContent = t("trialTitle", { days: trial.days || 3 });
      $("trial-claim").disabled = Boolean(trial.claiming);
      $("trial-claim").classList.toggle("is-busy", Boolean(trial.claiming));
      const trialNote = trial.activated ? t("trialActivated", { days: trial.days || 3 }) : window.PrimeServerText.translate(trial.error);
      $("trial-done").hidden = !trialNote;
      $("trial-done").textContent = trialNote || "";
      $("trial-done").classList.toggle("is-error", !trial.activated && Boolean(trial.error));
      $("subscription").hidden = state.subscription !== "none" || trialOffered;
      const account = state.billing.account;
      $("sub-chip").hidden = !account?.plan_name;
      if (account?.plan_name) {
        $("sub-chip-text").textContent = `${t("currentPlan", { name: account.plan_name })} · ${t("daysLeft", { days: account.remaining_days ?? 0 })}`;
      }
      const vpnError = state.vpn === "disconnected" && state.subscription !== "none" ? window.PrimeServerText.translate(state.vpn_error) : "";
      $("vpn-error").hidden = !vpnError;
      $("vpn-error").textContent = vpnError || "";
    }

    return { render };
  }

  window.PrimeDashboardView = { create };
})();
