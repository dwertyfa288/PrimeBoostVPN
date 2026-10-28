(function () {
  const CALL_TIMEOUT_MS = 10000;

  function withTimeout(promise) {
    return Promise.race([
      promise,
      new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), CALL_TIMEOUT_MS)),
    ]);
  }

  async function call(method, params) {
    if (!window.astra || typeof window.astra.callBackend !== "function") {
      throw new Error("Astra bridge is not available");
    }
    const result = await withTimeout(window.astra.callBackend(method, params || {}));
    if (typeof result === "string") {
      try {
        return JSON.parse(result);
      } catch {
        return result;
      }
    }
    return result;
  }

  window.PrimeBackend = { call };
})();
