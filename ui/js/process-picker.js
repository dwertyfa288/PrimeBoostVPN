(function () {
  const { t } = window.PrimeI18n;
  const $ = (id) => document.getElementById(id);

  function create({ call, onAdd, log }) {
    const modal = $("picker");
    document.body.appendChild(modal);
    const list = $("picker-list");
    const search = $("picker-search");
    const addButton = $("picker-add");
    let processes = [];
    let selected = new Set();

    function close(reason) {
      if (modal.hidden) return;
      modal.hidden = true;
      log(`picker closed: ${reason}`);
    }

    function updateButton() {
      addButton.disabled = selected.size === 0;
      addButton.querySelector(".pv-btn-text").textContent = t("addSelected", { count: selected.size });
    }

    function render() {
      const query = search.value.trim().toLowerCase();
      const visible = processes.filter(
        (entry) => !query || entry.name.toLowerCase().includes(query) || entry.path.toLowerCase().includes(query),
      );
      list.replaceChildren();
      if (visible.length === 0) {
        const empty = document.createElement("li");
        empty.className = "pv-empty";
        empty.textContent = t("noProcesses");
        list.append(empty);
        return;
      }
      for (const entry of visible) {
        const item = document.createElement("li");
        item.classList.toggle("is-checked", selected.has(entry.path));
        const check = document.createElement("span");
        check.className = "pv-check";
        const text = document.createElement("span");
        text.className = "pv-proc";
        const name = document.createElement("b");
        name.textContent = entry.name;
        const path = document.createElement("small");
        path.textContent = entry.path;
        text.append(name, path);
        item.append(check, text);
        item.addEventListener("click", () => {
          if (selected.has(entry.path)) selected.delete(entry.path);
          else selected.add(entry.path);
          item.classList.toggle("is-checked", selected.has(entry.path));
          updateButton();
        });
        list.append(item);
      }
    }

    async function open() {
      selected = new Set();
      search.value = "";
      search.placeholder = t("search");
      processes = [];
      list.replaceChildren();
      const loading = document.createElement("li");
      loading.className = "pv-empty";
      loading.append(Object.assign(document.createElement("span"), { className: "pv-dots" }));
      loading.firstChild.innerHTML = "<i></i><i></i><i></i>";
      list.append(loading);
      updateButton();
      modal.hidden = false;
      log("picker opened");
      const result = await call("list_processes");
      processes = Array.isArray(result) ? result : [];
      log(`picker loaded ${processes.length} processes, visible=${!modal.hidden}`);
      render();
    }

    search.addEventListener("input", render);
    $("picker-close").addEventListener("click", () => close("close button"));
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") close("escape");
    });
    addButton.addEventListener("click", () => {
      onAdd([...selected]);
      close("added");
    });

    return { open };
  }

  window.PrimeProcessPicker = { create };
})();
