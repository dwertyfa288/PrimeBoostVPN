(function () {
  function parseColor(value) {
    const probe = document.createElement("span");
    probe.style.color = value;
    document.body.appendChild(probe);
    const resolved = getComputedStyle(probe).color;
    probe.remove();
    const channels = resolved.match(/[\d.]+/g);
    return channels ? channels.slice(0, 3).map(Number) : null;
  }

  function isLight(rgb) {
    const [r, g, b] = rgb.map((channel) => {
      const c = channel / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.45;
  }

  function detect() {
    const root = document.documentElement;
    const attr = (root.getAttribute("data-theme") || "").toLowerCase();
    if (attr.includes("light")) return "light";
    if (attr.includes("dark")) return "dark";

    const surface = getComputedStyle(root).getPropertyValue("--color-surface").trim();
    const rgb = surface && parseColor(surface);
    if (rgb) return isLight(rgb) ? "light" : "dark";

    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }

  function apply() {
    document.body.dataset.pvTheme = detect();
  }

  function watch() {
    apply();
    new MutationObserver(apply).observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "style", "class"],
    });
    if (window.astra && typeof window.astra.onThemeChange === "function") {
      window.astra.onThemeChange(() => requestAnimationFrame(apply));
    }
  }

  window.PrimeTheme = { watch };
})();
