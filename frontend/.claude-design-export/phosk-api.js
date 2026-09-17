/* Phoskonomia — frontend API client. Hooks the UI to the real Rust backend
   (phosk_api, http://127.0.0.1:3000/api/v1). Every call surfaces its result as
   a toast; a 501 shows "NO IMPL YET" with the backend's todo tag. No mock
   fallback. Exposes window.phoskApi + window.phoskToast. Plain JS (no JSX) so it
   loads before any React component and is callable from any onClick. */
(function () {
  var BASE = window.PHOSK_API_BASE || "http://127.0.0.1:3000/api/v1";

  function toast(text, kind) {
    var host = document.getElementById("phosk-toasts");
    if (!host) {
      host = document.createElement("div");
      host.id = "phosk-toasts";
      host.style.cssText =
        "position:fixed;right:16px;bottom:16px;z-index:99999;display:flex;" +
        "flex-direction:column;gap:8px;max-width:400px;" +
        "font-family:ui-monospace,SFMono-Regular,Menlo,monospace;";
      document.body.appendChild(host);
    }
    var tone = kind === "err" ? "#ff5d73" : kind === "ok" ? "#56e39f" : "#8b7bff";
    var el = document.createElement("div");
    el.style.cssText =
      "background:rgba(8,5,20,.92);border:2px solid " + tone + ";color:#e7e7f2;" +
      "padding:10px 12px;border-radius:0;font-size:12px;line-height:1.4;" +
      "letter-spacing:.02em;box-shadow:0 8px 30px rgba(0,0,0,.5);white-space:pre-wrap;";
    el.textContent = text;
    host.appendChild(el);
    setTimeout(function () {
      el.style.transition = "opacity .4s";
      el.style.opacity = "0";
      setTimeout(function () { el.remove(); }, 400);
    }, 4200);
  }

  function qs(query) {
    if (!query) return "";
    var parts = Object.keys(query)
      .filter(function (k) {
        return query[k] !== undefined && query[k] !== null && query[k] !== "";
      })
      .map(function (k) {
        return encodeURIComponent(k) + "=" + encodeURIComponent(query[k]);
      });
    return parts.length ? "?" + parts.join("&") : "";
  }

  async function call(method, path, body) {
    var opts = { method: method, headers: {} };
    if (body !== undefined && body !== null) {
      opts.headers["Content-Type"] = "application/json";
      opts.body = JSON.stringify(body);
    }
    try {
      var res = await fetch(BASE + path, opts);
      var data = null;
      try { data = await res.json(); } catch (e) {}
      if (res.status === 501) {
        toast("NO IMPL YET — " + method + " " + path + "\n" + ((data && data.todo) || ""), "warn");
      } else if (res.ok) {
        toast("OK — " + method + " " + path, "ok");
      } else {
        toast("ERR " + res.status + " — " + method + " " + path, "err");
      }
      return { status: res.status, data: data };
    } catch (e) {
      toast("NO BACKEND — " + method + " " + path + "\nstart it: cargo run -p phosk_api", "err");
      return { status: 0, error: String(e) };
    }
  }

  window.phoskToast = toast;
  window.phoskApi = {
    base: BASE,
    get: function (path, query) { return call("GET", path + qs(query)); },
    post: function (path, body) { return call("POST", path, body); },
    patch: function (path, body) { return call("PATCH", path, body); },
    put: function (path, body) { return call("PUT", path, body); },
    del: function (path) { return call("DELETE", path); },
  };
})();
