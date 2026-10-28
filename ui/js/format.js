(function () {
  const { t } = window.PrimeI18n;

  function scaled(value, units) {
    let amount = Math.max(0, Number(value) || 0);
    let index = 0;
    while (amount >= 1024 && index < units.length - 1) {
      amount /= 1024;
      index += 1;
    }
    const digits = amount >= 100 || index === 0 ? 0 : 1;
    return `${amount.toFixed(digits)} ${units[index]}`;
  }

  function speed(bytesPerSecond) {
    return scaled(bytesPerSecond, t("unitSpeed"));
  }

  function size(bytes) {
    return scaled(bytes, t("unitSize"));
  }

  function duration(totalSeconds) {
    const seconds = Math.max(0, Math.floor(totalSeconds || 0));
    const pad = (n) => String(n).padStart(2, "0");
    return `${pad(Math.floor(seconds / 3600))}:${pad(Math.floor((seconds % 3600) / 60))}:${pad(seconds % 60)}`;
  }

  window.PrimeFormat = { speed, size, duration };
})();
