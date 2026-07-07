import type { HostRouteResult } from "./host-route-results.js";

export function isAdminPanelRoute(
  pathname: string,
  staticEnabled: boolean,
): boolean {
  return (
    pathname === "/admin" ||
    pathname === "/admin/" ||
    (!staticEnabled && pathname === "/")
  );
}

export function adminPanelResponse(authRequired: boolean): HostRouteResult {
  return htmlResponse(adminPanelHtml(authRequired));
}

function htmlResponse(body: string): HostRouteResult {
  return {
    status: 200,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    },
    body,
  };
}

function adminPanelHtml(authRequired: boolean): string {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Rusty Crew Admin</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f6f7f9;
      --panel: #ffffff;
      --panel-strong: #eef2f6;
      --text: #17202a;
      --muted: #607083;
      --border: #d7dee7;
      --good: #147a4a;
      --warn: #9c5a00;
      --bad: #b42318;
      --accent: #2457a6;
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 14px;
      line-height: 1.45;
    }

    header {
      background: var(--panel);
      border-bottom: 1px solid var(--border);
    }

    .shell {
      width: min(1180px, calc(100% - 32px));
      margin: 0 auto;
    }

    .topbar {
      display: grid;
      grid-template-columns: minmax(180px, 1fr) minmax(280px, 520px);
      gap: 20px;
      align-items: center;
      padding: 22px 0;
    }

    h1 {
      margin: 0;
      font-size: 24px;
      font-weight: 700;
      letter-spacing: 0;
    }

    .subtitle {
      margin: 4px 0 0;
      color: var(--muted);
    }

    .token-row {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 8px;
    }

    input {
      min-width: 0;
      height: 38px;
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 0 10px;
      font: inherit;
      background: #fff;
      color: var(--text);
    }

    button {
      height: 38px;
      border: 1px solid #1f4f95;
      border-radius: 6px;
      padding: 0 14px;
      background: var(--accent);
      color: #fff;
      font: inherit;
      font-weight: 650;
      cursor: pointer;
    }

    button.secondary {
      border-color: var(--border);
      background: #fff;
      color: var(--text);
    }

    main {
      padding: 20px 0 32px;
    }

    .status-line {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-bottom: 14px;
      color: var(--muted);
    }

    .pill {
      display: inline-flex;
      align-items: center;
      min-height: 26px;
      border: 1px solid var(--border);
      border-radius: 999px;
      padding: 3px 10px;
      background: var(--panel);
      color: var(--muted);
      font-size: 13px;
    }

    .pill.good {
      border-color: #9fd7b8;
      color: var(--good);
      background: #eefaf3;
    }

    .pill.warn {
      border-color: #f0c982;
      color: var(--warn);
      background: #fff7e8;
    }

    .pill.bad {
      border-color: #f1a39d;
      color: var(--bad);
      background: #fff1f0;
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(12, 1fr);
      gap: 12px;
    }

    .panel {
      grid-column: span 6;
      background: var(--panel);
      border: 1px solid var(--border);
      border-radius: 8px;
      overflow: hidden;
    }

    .panel.wide {
      grid-column: span 12;
    }

    .panel h2 {
      margin: 0;
      padding: 12px 14px;
      border-bottom: 1px solid var(--border);
      background: var(--panel-strong);
      font-size: 15px;
      letter-spacing: 0;
    }

    .panel-body {
      padding: 12px 14px;
    }

    .metrics {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
      gap: 8px;
    }

    .metric {
      min-height: 72px;
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 10px;
      background: #fbfcfd;
    }

    .metric span {
      display: block;
      color: var(--muted);
      font-size: 12px;
    }

    .metric strong {
      display: block;
      margin-top: 4px;
      font-size: 20px;
      font-weight: 720;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }

    th,
    td {
      border-bottom: 1px solid var(--border);
      padding: 8px 6px;
      text-align: left;
      vertical-align: top;
      overflow-wrap: anywhere;
    }

    th {
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
    }

    tr:last-child td {
      border-bottom: 0;
    }

    .empty,
    .error {
      color: var(--muted);
      padding: 12px 0;
    }

    .error {
      color: var(--bad);
    }

    pre {
      max-height: 280px;
      overflow: auto;
      margin: 0;
      padding: 10px;
      border: 1px solid var(--border);
      border-radius: 6px;
      background: #101820;
      color: #e8eef5;
      font-size: 12px;
      line-height: 1.5;
      white-space: pre-wrap;
    }

    @media (max-width: 800px) {
      .topbar,
      .token-row {
        grid-template-columns: 1fr;
      }

      .panel {
        grid-column: span 12;
      }

      button {
        width: 100%;
      }
    }
  </style>
</head>
<body>
  <header>
    <div class="shell topbar">
      <div>
        <h1>Rusty Crew Admin</h1>
        <p class="subtitle">Service diagnostics for the local field-test runtime</p>
      </div>
      <form id="tokenForm" class="token-row"${authRequired ? "" : " hidden"}>
        <input id="tokenInput" name="token" type="password" autocomplete="current-password" placeholder="Admin bearer token">
        <button type="submit">Refresh</button>
        <button id="clearToken" class="secondary" type="button">Clear</button>
      </form>
    </div>
  </header>
  <main class="shell">
    <div class="status-line" id="statusLine"></div>
    <section class="grid">
      <article class="panel wide">
        <h2>Overview</h2>
        <div class="panel-body">
          <div class="metrics" id="overviewMetrics"></div>
        </div>
      </article>
      <article class="panel">
        <h2>Persistence</h2>
        <div class="panel-body" id="persistencePanel"></div>
      </article>
      <article class="panel">
        <h2>Queues And Health</h2>
        <div class="panel-body" id="healthPanel"></div>
      </article>
      <article class="panel">
        <h2>Channels</h2>
        <div class="panel-body" id="channelsPanel"></div>
      </article>
      <article class="panel">
        <h2>MCP</h2>
        <div class="panel-body" id="mcpPanel"></div>
      </article>
      <article class="panel wide">
        <h2>Recent Events</h2>
        <div class="panel-body" id="eventsPanel"></div>
      </article>
      <article class="panel wide">
        <h2>Raw Diagnostics</h2>
        <div class="panel-body"><pre id="rawPanel">Waiting for diagnostics...</pre></div>
      </article>
    </section>
  </main>
  <script>
    (function () {
      var authRequired = ${authRequired ? "true" : "false"};
      var tokenInput = document.getElementById("tokenInput");
      var statusLine = document.getElementById("statusLine");
      var savedToken = authRequired ? (localStorage.getItem("rustyCrewAdminToken") || "") : "";
      tokenInput.value = savedToken;

      document.getElementById("tokenForm").addEventListener("submit", function (event) {
        event.preventDefault();
        var token = tokenInput.value.trim();
        if (token) localStorage.setItem("rustyCrewAdminToken", token);
        refresh();
      });

      document.getElementById("clearToken").addEventListener("click", function () {
        localStorage.removeItem("rustyCrewAdminToken");
        tokenInput.value = "";
        refresh();
      });

      function headers() {
        var token = tokenInput.value.trim();
        return authRequired && token ? { authorization: "Bearer " + token } : {};
      }

      async function api(path, auth) {
        var response = await fetch(path, { headers: auth ? headers() : {} });
        var body = await response.json();
        if (!response.ok || !body.ok) {
          var message = body.error ? body.error.message : response.statusText;
          throw new Error(message || ("request failed: " + response.status));
        }
        return body.data;
      }

      function pill(text, kind) {
        var span = document.createElement("span");
        span.className = "pill " + (kind || "");
        span.textContent = text;
        return span;
      }

      function setStatus(items) {
        statusLine.replaceChildren.apply(statusLine, items);
      }

      function metric(label, value) {
        var node = document.createElement("div");
        node.className = "metric";
        var labelNode = document.createElement("span");
        labelNode.textContent = label;
        var valueNode = document.createElement("strong");
        valueNode.textContent = value === undefined || value === null ? "n/a" : String(value);
        node.append(labelNode, valueNode);
        return node;
      }

      function renderMetrics(id, entries) {
        var target = document.getElementById(id);
        target.replaceChildren.apply(target, entries.map(function (entry) {
          return metric(entry[0], entry[1]);
        }));
      }

      function renderObjectTable(id, data) {
        var target = document.getElementById(id);
        if (!data) {
          target.innerHTML = '<div class="empty">No data reported.</div>';
          return;
        }
        var table = document.createElement("table");
        Object.keys(data).sort().forEach(function (key) {
          var row = document.createElement("tr");
          var name = document.createElement("th");
          var value = document.createElement("td");
          name.textContent = key;
          value.textContent = typeof data[key] === "object" ? JSON.stringify(data[key]) : String(data[key]);
          row.append(name, value);
          table.append(row);
        });
        target.replaceChildren(table);
      }

      function renderItemsTable(id, items, columns) {
        var target = document.getElementById(id);
        if (!items || items.length === 0) {
          target.innerHTML = '<div class="empty">No records reported.</div>';
          return;
        }
        var table = document.createElement("table");
        var head = document.createElement("tr");
        columns.forEach(function (column) {
          var th = document.createElement("th");
          th.textContent = column.label;
          head.append(th);
        });
        table.append(head);
        items.forEach(function (item) {
          var row = document.createElement("tr");
          columns.forEach(function (column) {
            var td = document.createElement("td");
            var value = column.value(item);
            td.textContent = value === undefined || value === null || value === "" ? "n/a" : String(value);
            row.append(td);
          });
          table.append(row);
        });
        target.replaceChildren(table);
      }

      function setPanelError(id, error) {
        document.getElementById(id).innerHTML = '<div class="error">' + escapeHtml(error.message || String(error)) + '</div>';
      }

      function escapeHtml(value) {
        return String(value).replace(/[&<>"']/g, function (char) {
          return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char];
        });
      }

      async function refresh() {
        var token = tokenInput.value.trim();
        setStatus([pill("Loading", "warn")]);
        try {
          var health = await api("/v1/admin/healthz", false);
          var statusPills = [
            pill("Liveness: " + health.status, health.status === "live" ? "good" : "bad")
          ];
          if (authRequired && !token) {
            setStatus(statusPills.concat([pill("Enter token for diagnostics", "warn")]));
            return;
          }

          var results = await Promise.allSettled([
            api("/v1/admin/readyz", true),
            api("/v1/admin/diagnostics", true),
            api("/v1/admin/diagnostics/persistence", true),
            api("/v1/admin/diagnostics/channels", true),
            api("/v1/admin/diagnostics/mcp", true),
            api("/v1/admin/events/recent", true)
          ]);

          var ready = unwrap(results[0]);
          var diagnostics = unwrap(results[1]);
          var persistence = unwrap(results[2]);
          var channels = unwrap(results[3]);
          var mcp = unwrap(results[4]);
          var events = unwrap(results[5]);
          var overview = diagnostics.overview || {};
          var summary = overview.summary || {};

          statusPills.push(pill("Readiness: " + ready.status, ready.status === "ready" ? "good" : "warn"));
          statusPills.push(pill("Generated: " + (overview.generatedAt || "n/a")));
          if (overview.degraded) statusPills.push(pill("Degraded", "warn"));
          setStatus(statusPills);

          renderMetrics("overviewMetrics", [
            ["Sessions", summary.sessions],
            ["Active", summary.activeSessions],
            ["Idle", summary.idleSessions],
            ["Queued", summary.queueDepth],
            ["Agents", summary.agents],
            ["Tools", summary.tools],
            ["Recent errors", summary.recentErrors]
          ]);

          renderObjectTable("persistencePanel", Object.assign({}, persistence, {
            tableCounts: JSON.stringify((persistence && persistence.tableCounts) || {})
          }));

          renderObjectTable("healthPanel", {
            runtimeHealth: overview.health,
            degraded: overview.degraded,
            reasonCodes: (overview.reasonCodes || []).join(", ") || "none",
            queues: overview.queues ? JSON.stringify(overview.queues) : "none"
          });

          renderItemsTable("channelsPanel", channels.items || [], [
            { label: "Binding", value: function (item) { return item.bindingId; } },
            { label: "Agent", value: function (item) { return item.agentId; } },
            { label: "Status", value: function (item) { return item.status; } },
            { label: "Channel", value: function (item) { return item.externalChannelId; } }
          ]);

          renderItemsTable("mcpPanel", mcp.items || [], [
            { label: "Binding", value: function (item) { return item.bindingId; } },
            { label: "Agent", value: function (item) { return item.agentId; } },
            { label: "Status", value: function (item) { return item.status; } },
            { label: "Servers", value: function (item) { return (item.serverNames || []).join(", "); } }
          ]);

          renderItemsTable("eventsPanel", events.items || [], [
            { label: "Time", value: function (item) { return item.createdAt; } },
            { label: "Source", value: function (item) { return item.source; } },
            { label: "Type", value: function (item) { return item.eventType; } },
            { label: "Summary", value: function (item) { return item.summary; } }
          ]);

          document.getElementById("rawPanel").textContent = JSON.stringify(diagnostics, null, 2);
        } catch (error) {
          setStatus([pill("Diagnostics error", "bad"), pill(error.message || String(error), "bad")]);
          setPanelError("healthPanel", error);
        }
      }

      function unwrap(result) {
        if (result.status === "fulfilled") return result.value;
        throw result.reason;
      }

      refresh();
      setInterval(refresh, 15000);
    }());
  </script>
</body>
</html>`;
}
