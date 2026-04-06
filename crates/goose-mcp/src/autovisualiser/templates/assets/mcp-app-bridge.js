/**
 * MCP App Bridge — shared protocol layer for autovisualiser templates.
 *
 * Provides:
 *   McpAppBridge.init(options)  → bootstraps the MCP Apps lifecycle
 *
 * options:
 *   appName      – string, e.g. "autovisualiser-chart"
 *   onData       – function(data): called when tool-result or tool-input arrives
 *   onTheme      – function(): called after theme CSS vars are applied (optional)
 *   extractData  – function(msg): custom data extractor (optional, has sensible default)
 */
var McpAppBridge = (function () {
  "use strict";

  var _nextId = 1;
  var _currentData = null;
  var _onData = null;
  var _onTheme = null;
  var _extractData = null;

  // ── JSON-RPC helpers ────────────────────────────────────────────────

  function sendRequest(method, params) {
    return new Promise(function (resolve, reject) {
      var id = _nextId++;
      function handler(event) {
        if (event.data && event.data.id === id) {
          window.removeEventListener("message", handler);
          if (event.data.result) resolve(event.data.result);
          else if (event.data.error) reject(event.data.error);
        }
      }
      window.addEventListener("message", handler);
      window.parent.postMessage(
        { jsonrpc: "2.0", id: id, method: method, params: params },
        "*"
      );
    });
  }

  function sendNotification(method, params) {
    window.parent.postMessage(
      { jsonrpc: "2.0", method: method, params: params },
      "*"
    );
  }

  // ── Size reporting ──────────────────────────────────────────────────

  function reportSize() {
    // In fullscreen/pip the host controls sizing — skip size reports
    // to avoid a feedback loop when transitioning back to inline.
    if (_displayMode === "fullscreen" || _displayMode === "pip") return;

    var h = Math.max(
      document.body.scrollHeight,
      document.body.offsetHeight,
      document.documentElement.scrollHeight,
      document.documentElement.offsetHeight
    );
    sendNotification("ui/notifications/size-changed", {
      width: document.body.scrollWidth,
      height: h,
    });
  }

  // ── Theming ─────────────────────────────────────────────────────────

  var _displayMode = "inline";

  function applyTheme(hostContext) {
    if (!hostContext) return;
    // Clear resolved color cache — theme change means light-dark() flips.
    _probeCache = {};
    if (hostContext.theme)
      document.documentElement.style.colorScheme = hostContext.theme;
    if (hostContext.displayMode) {
      _displayMode = hostContext.displayMode;
      document.documentElement.setAttribute("data-display-mode", _displayMode);
    }
    if (hostContext.styles && hostContext.styles.variables) {
      var vars = hostContext.styles.variables;
      for (var key in vars) {
        if (vars[key]) document.documentElement.style.setProperty(key, vars[key]);
      }
    }
    if (hostContext.styles && hostContext.styles.css && hostContext.styles.css.fonts) {
      if (!document.getElementById("mcp-host-fonts")) {
        var style = document.createElement("style");
        style.id = "mcp-host-fonts";
        style.textContent = hostContext.styles.css.fonts;
        document.head.appendChild(style);
      }
    }
    if (_onTheme) _onTheme();
  }

  // ── Default data extractor ──────────────────────────────────────────

  function defaultExtractData(msg) {
    var sc = msg.params && msg.params.structuredContent;
    if (sc) {
      if (sc.data) return sc.data;
      return sc;
    }
    var args = msg.params && msg.params.arguments;
    if (args) {
      if (args.data) return args.data;
      return args;
    }
    return null;
  }

  // ── Read a computed CSS variable ────────────────────────────────────

  // Host tokens may use light-dark(light, dark) syntax. getComputedStyle
  // returns the raw string for custom properties, so we resolve it by
  // reading the value through a real CSS property on a hidden probe element.
  var _probe = null;
  var _probeCache = {};

  function resolveColor(raw) {
    if (!raw) return raw;
    // Fast path: no light-dark() wrapper
    if (raw.indexOf("light-dark(") === -1) return raw;

    var cached = _probeCache[raw];
    if (cached) return cached;

    if (!_probe) {
      _probe = document.createElement("div");
      _probe.style.cssText = "position:absolute;width:0;height:0;overflow:hidden;pointer-events:none;";
      document.body.appendChild(_probe);
    }
    _probe.style.color = raw;
    var resolved = getComputedStyle(_probe).color;
    _probeCache[raw] = resolved;
    return resolved;
  }

  function cssVar(name) {
    var raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return resolveColor(raw);
  }

  // ── Public API ──────────────────────────────────────────────────────

  function init(options) {
    _onData = options.onData;
    _onTheme = options.onTheme || null;
    _extractData = options.extractData || defaultExtractData;

    // Size observation
    if (typeof ResizeObserver !== "undefined") {
      new ResizeObserver(reportSize).observe(document.body);
    }
    window.addEventListener("resize", reportSize);

    // Message listener
    window.addEventListener("message", function (event) {
      var msg = event.data;
      if (!msg || msg.jsonrpc !== "2.0") return;

      if (
        msg.method === "ui/notifications/tool-result" ||
        msg.method === "ui/notifications/tool-input"
      ) {
        var data = _extractData(msg);
        if (data) {
          _currentData = data;
          _onData(data);
        }
      }

      if (msg.method === "ui/notifications/host-context-changed") {
        applyTheme(msg.params);
      }

      if (msg.method === "ui/resource-teardown" && msg.id) {
        window.parent.postMessage(
          { jsonrpc: "2.0", id: msg.id, result: {} },
          "*"
        );
      }
    });

    // Handshake
    var appIdentity = {
      name: options.appName || "autovisualiser",
      version: "1.0.0",
    };
    sendRequest("ui/initialize", {
      protocolVersion: "2026-01-26",
      clientInfo: appIdentity,
      appCapabilities: {
        availableDisplayModes: ["inline", "fullscreen"],
      },
    })
      .then(function (result) {
        applyTheme(result.hostContext || result);
        sendNotification("ui/notifications/initialized", {});
        reportSize();
      })
      .catch(function (err) {
        console.warn("[" + (options.appName || "autovisualiser") + "] init failed:", err);
        reportSize();
      });
  }

  function positionTooltip(tooltipEl, event, offsetX, offsetY) {
    var ox = offsetX || 10;
    var oy = offsetY || -10;
    var el = tooltipEl instanceof HTMLElement ? tooltipEl : tooltipEl.node();
    if (!el) return;
    var rect = el.getBoundingClientRect();
    var w = rect.width || 150;
    var h = rect.height || 40;
    var x = event.pageX + ox;
    var y = event.pageY + oy;
    if (x + w > window.innerWidth - 8) x = event.pageX - w - ox;
    if (y + h > window.innerHeight - 8) y = event.pageY - h - Math.abs(oy);
    if (x < 8) x = 8;
    if (y < 8) y = 8;
    el.style.left = x + "px";
    el.style.top = y + "px";
  }

  /**
   * Display an error message in the page body.
   * Hides the loading indicator and shows a styled error box.
   */
  function showError(message) {
    var loader = document.getElementById("loadingIndicator");
    if (loader) loader.classList.add("hidden");

    var el = document.createElement("div");
    el.style.cssText =
      "padding:24px;text-align:center;color:" +
      (cssVar("--color-text-secondary") || "#878787") +
      ";font-size:14px;font-family:" +
      (cssVar("--font-sans") || "sans-serif");
    el.textContent = "Unable to render visualization: " + message;
    document.body.appendChild(el);
  }

  return {
    init: init,
    cssVar: cssVar,
    reportSize: reportSize,
    positionTooltip: positionTooltip,
    showError: showError,
    get currentData() {
      return _currentData;
    },
    get displayMode() {
      return _displayMode;
    },
  };
})();
