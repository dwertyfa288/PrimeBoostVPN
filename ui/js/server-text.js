(function () {
  const ENGLISH = [
    ["Сессия истекла, войдите снова", "Session expired, please sign in again"],
    ["Нет активной подписки", "No active subscription"],
    ["Нет связи с сервером: {0}", "Cannot reach the server: {0}"],
    ["Некорректный ответ сервера", "Unexpected server response"],
    ["Ошибка сервера", "Server error"],
    ["Ошибка проверки подписки", "Subscription check failed"],
    ["Сервер вернул некорректный ключ", "The server returned an invalid key"],
    ["Сервер ключей ответил {0}", "Key server responded {0}"],
    ["Не удалось начать вход через Telegram", "Could not start Telegram sign-in"],
    ["Ссылка устарела, попробуйте снова", "The link expired, please try again"],
    ["Время ожидания истекло", "Timed out"],
    ["Адрес не разрешён", "Address not allowed"],
    ["Не удалось открыть браузер: {0}", "Could not open the browser: {0}"],
    ["Некорректная ссылка оплаты", "Invalid payment link"],
    ["Войдите в аккаунт", "Please sign in"],
    ["Не удалось загрузить тарифы", "Could not load plans"],
    ["Некорректный список тарифов", "Invalid plan list"],
    ["Не удалось рассчитать стоимость", "Could not calculate the price"],
    ["Сервер не вернул стоимость", "The server returned no price"],
    ["Не удалось создать платёж", "Could not create the payment"],
    ["Сервер не вернул номер заказа", "The server returned no order number"],
    ["Не удалось оплатить с баланса", "Could not pay from balance"],
    ["Платёж не найден", "Payment not found"],
    ["Реквизиты оплаты недоступны", "Payment details are unavailable"],
    ["Реквизиты оплаты не готовы, попробуйте ещё раз", "Payment details are not ready, try again"],
    ["Ссылка оплаты ещё не готова", "The payment link is not ready yet"],
    ["Время ожидания оплаты истекло", "Payment timed out"],
    ["Не удалось активировать пробный период", "Could not activate the trial"],
    ["Пробный период уже использован на этом аккаунте", "The trial was already used on this account"],
    ["Пробный период уже использован на этом устройстве", "The trial was already used on this device"],
    ["Не удалось определить устройство", "Could not identify this device"],
    ["Пробный период недоступен", "The trial is unavailable"],
    ["Проверка подписки", "Checking the subscription"],
    ["Подбор лучшего сервера", "Finding the best server"],
    ["Запрос прав администратора", "Requesting administrator rights"],
    ["Подключение · {0}", "Connecting · {0}"],
    ["Проверка соединения: {0}", "Checking the connection: {0}"],
    ["Нет доступных серверов", "No servers available"],
    ["Не удалось подключиться ни к одному серверу", "Could not connect to any server"],
    ["Сервер не отвечает или ключ не работает", "The server is not responding or the key does not work"],
    ["Сервер не отвечает", "The server is not responding"],
    ["Не удалось найти адрес сервера", "Could not find the server address"],
    ["Ядро XHTTP завершилось при запуске", "The XHTTP core exited on start"],
    ["Ядро XHTTP не открыло локальный порт", "The XHTTP core did not open its local port"],
    ["Соединение прервано: {0}", "Connection lost: {0}"],
    ["Соединение прервано", "Connection lost"],
    ["Ядро VPN завершилось при запуске: {0}", "The VPN core exited on start: {0}"],
    ["Ядро VPN завершилось при запуске", "The VPN core exited on start"],
    ["Ядро VPN остановилось: {0}", "The VPN core stopped: {0}"],
    ["Ядро VPN остановилось", "The VPN core stopped"],
    ["Не найдено ядро VPN: {0}", "VPN core not found: {0}"],
    ["Не удалось запустить ядро VPN: {0}", "Could not start the VPN core: {0}"],
    ["Не найден помощник запуска: {0}", "Launch helper not found: {0}"],
    ["Не удалось запустить ядро с правами администратора (код {0})", "Could not start the core as administrator (code {0})"],
    ["Нужны права администратора: подтверди запрос Windows", "Administrator rights needed: confirm the Windows prompt"],
    ["Нет доступа к stdin ядра", "No access to the core's stdin"],
    ["Не удалось передать конфигурацию: {0}", "Could not pass the configuration: {0}"],
    ["Не удалось подготовить конфигурацию: {0}", "Could not prepare the configuration: {0}"],
    ["Нет свободного порта: {0}", "No free port: {0}"],
    ["Не удалось включить системный прокси: {0}", "Could not enable the system proxy: {0}"],
    ["Не удалось включить системный прокси через gsettings", "Could not enable the system proxy via gsettings"],
    ["Не удалось открыть настройки прокси: {0}", "Could not open proxy settings: {0}"],
    ["Права для TUN не выданы", "TUN rights were not granted"],
    ["Не найден pkexec. Выполни: {0}", "pkexec not found. Run: {0}"],
    ["Не удалось запустить pkexec: {0}", "Could not start pkexec: {0}"],
    ["Не найден setcap: установи libcap2-bin (Debian/Ubuntu) или libcap", "setcap not found: install libcap2-bin (Debian/Ubuntu) or libcap"],
    ["Права не применились: файловая система смонтирована с nosuid?", "Rights were not applied: is the filesystem mounted with nosuid?"],
    ["Ключ должен начинаться с vless://", "The key must start with vless://"],
    ["В ключе нет UUID", "The key has no UUID"],
    ["Некорректный UUID в ключе", "Invalid UUID in the key"],
  ];

  const COUNTRIES = { Нидерланды: "Netherlands", Польша: "Poland", Германия: "Germany", США: "USA" };

  const escape =(text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const PATTERNS = ENGLISH.map(([ru, en]) => ({
    pattern: new RegExp(`^${escape(ru).replace("\\{0\\}", "([\\s\\S]*)")}$`),
    en,
  }));

  function translate(message) {
    if (!message || window.PrimeI18n.language === "ru") return message || "";
    const text = String(message);
    for (const { pattern, en } of PATTERNS) {
      const match = text.match(pattern);
      if (match) return en.replace("{0}", COUNTRIES[match[1]] ?? match[1] ?? "");
    }
    return text;
  }

  window.PrimeServerText = { translate };
})();
