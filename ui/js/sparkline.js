(function () {
  const WIDTH = 120;
  const HEIGHT = 36;
  const SAMPLES = 40;

  function create(lineNode, areaNode) {
    let values = [];

    function draw() {
      if (values.length < 2) {
        lineNode.setAttribute("d", "");
        areaNode.setAttribute("d", "");
        return;
      }
      const max = Math.max(...values, 1);
      const stepX = WIDTH / (values.length - 1);
      const points = values.map((value, index) => [
        index * stepX,
        HEIGHT - 2 - (value / max) * (HEIGHT - 8),
      ]);
      let line = `M${points[0][0].toFixed(1)} ${points[0][1].toFixed(1)}`;
      for (let i = 1; i < points.length; i += 1) {
        const [x0, y0] = points[i - 1];
        const [x1, y1] = points[i];
        const mid = (x0 + x1) / 2;
        line += ` C${mid.toFixed(1)} ${y0.toFixed(1)} ${mid.toFixed(1)} ${y1.toFixed(1)} ${x1.toFixed(1)} ${y1.toFixed(1)}`;
      }
      lineNode.setAttribute("d", line);
      areaNode.setAttribute(
        "d",
        `${line} L${points[points.length - 1][0].toFixed(1)} ${HEIGHT} L${points[0][0].toFixed(1)} ${HEIGHT} Z`,
      );
    }

    function push(value) {
      values.push(Math.max(0, Number(value) || 0));
      if (values.length > SAMPLES) values.shift();
      draw();
    }

    function clear() {
      values = [];
      draw();
    }

    return { push, clear };
  }

  window.PrimeSparkline = { create };
})();
