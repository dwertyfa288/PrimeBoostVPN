(function () {
  const { t } = window.PrimeI18n;
  const motion = window.PrimeMotion;
  const $ = (id) => document.getElementById(id);
  const MODE_HINTS = { bypass: "modeBypassHint", only: "modeOnlyHint", routing: "modeRoutingHint" };

  function basename(target) {
    const parts = target.trim().split(/[\\/]/);
    return parts[parts.length - 1] || target.trim();
  }

  function create({ call, refresh, log }) {
    const splitCard = document.querySelector(".pv-split-card");
    const modeBox = $("split-mode");
    let settings = null;
    let pending = false;
    let adminPromptArmed = false;
    const adminModal = window.PrimeAdminModal.create();

    function clone(value) {
      return JSON.parse(JSON.stringify(value));
    }

    async function save(next) {
      settings = next;
      pending = true;
      paint();
      await call("set_settings", next);
      pending = false;
      refresh();
    }

    function update(mutator) {
      if (!settings) return;
      const next = clone(settings);
      mutator(next);
      save(next);
    }

    document.querySelectorAll(".pv-switch").forEach((toggle) =>
      toggle.addEventListener("click", () => {
        if (toggle.dataset.setting !== "ipv6") adminPromptArmed = true;
        update((next) => {
          if (toggle.dataset.setting === "split_enabled") next.split.enabled = !next.split.enabled;
          else next[toggle.dataset.setting] = !next[toggle.dataset.setting];
        });
      }),
    );

    modeBox.querySelectorAll("[data-mode]").forEach((button) =>
      button.addEventListener("click", () => update((next) => (next.split.mode = button.dataset.mode))),
    );

    function addItems(key, values) {
      update((next) => {
        const existing = new Set(next.split[key].map((item) => item.toLowerCase()));
        for (const value of values) {
          const trimmed = value.trim();
          if (trimmed && !existing.has(trimmed.toLowerCase())) {
            next.split[key].push(trimmed);
            existing.add(trimmed.toLowerCase());
          }
        }
      });
    }

    function bindForm(formId, inputId, key) {
      $(formId).addEventListener("submit", (event) => {
        event.preventDefault();
        const input = $(inputId);
        if (!input.value.trim()) {
          motion.replay(input, "is-invalid");
          return;
        }
        addItems(key, [input.value]);
        input.value = "";
      });
    }

    bindForm("app-form", "app-input", "apps");
    bindForm("site-form", "site-input", "sites");

    const picker = window.PrimeProcessPicker.create({ call, log, onAdd: (paths) => addItems("apps", paths) });
    $("pick-apps").addEventListener("click", () => picker.open());

    function renderChips(listId, items, emptyKey, key, label) {
      const list = $(listId);
      const current = [...list.querySelectorAll("li[data-value]")].map((node) => node.dataset.value);
      if (current.join("\n") === items.join("\n") && list.childElementCount) return;
      list.replaceChildren();
      if (items.length === 0) {
        const empty = document.createElement("li");
        empty.className = "pv-empty";
        empty.textContent = t(emptyKey);
        list.append(empty);
        return;
      }
      for (const value of items) {
        const chip = document.createElement("li");
        chip.dataset.value = value;
        chip.title = value;
        const text = document.createElement("span");
        text.textContent = label(value);
        const remove = document.createElement("button");
        remove.type = "button";
        remove.textContent = "×";
        remove.addEventListener("click", () => {
          chip.classList.add("is-leaving");
          setTimeout(() => update((next) => (next.split[key] = next.split[key].filter((item) => item !== value))), 200);
        });
        chip.append(text, remove);
        list.append(chip);
      }
    }

    function paint() {
      if (!settings) return;
      const setSwitch = (name, on) => {
        const node = document.querySelector(`.pv-switch[data-setting="${name}"]`);
        node.setAttribute("aria-checked", String(Boolean(on)));
      };
      setSwitch("ipv6", settings.ipv6);
      setSwitch("tun_mode", settings.tun_mode);
      setSwitch("split_enabled", settings.split.enabled);
      splitCard.classList.toggle("is-open", settings.split.enabled);

      modeBox.dataset.active = settings.split.mode;
      modeBox.querySelectorAll("[data-mode]").forEach((button) =>
        button.classList.toggle("is-active", button.dataset.mode === settings.split.mode),
      );
      motion.swapText($("split-mode-hint"), t(MODE_HINTS[settings.split.mode]));

      renderChips("app-list", settings.split.apps, "noApps", "apps", basename);
      renderChips("site-list", settings.split.sites, "noSites", "sites", (value) => value);
    }

    function render(state) {
      $("app-input").placeholder = t("appPlaceholder");
      $("site-input").placeholder = t("sitePlaceholder");
      if (!pending) settings = clone(state.settings);
      paint();
      if (adminPromptArmed && !pending) {
        adminPromptArmed = false;
        if (state.needs_admin) adminModal.show(t(state.platform === "linux" ? "adminNoteLinux" : "adminNoteWindows"));
      }
    }

    return { render };
  }

  window.PrimeSettingsView = { create };
})();
