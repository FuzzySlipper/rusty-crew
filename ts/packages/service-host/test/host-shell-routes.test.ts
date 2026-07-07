import assert from "node:assert/strict";
import type { IncomingMessage } from "node:http";
import test from "node:test";

import {
  adminPanelResponse,
  isAdminPanelRoute,
} from "../src/admin-panel-routes.js";
import { handleHostShellRequest } from "../src/host-shell-routes.js";
import {
  handleStaticSiteRequest,
  resolveStaticSitePath,
  staticCacheControl,
  staticContentType,
  staticSiteRootFromPaths,
} from "../src/static-site-routes.js";

test("admin panel route helpers render the static diagnostics shell", () => {
  assert.equal(isAdminPanelRoute("/admin", true), true);
  assert.equal(isAdminPanelRoute("/", false), true);
  assert.equal(isAdminPanelRoute("/", true), false);

  const response = adminPanelResponse(true);
  if ("kind" in response) {
    assert.fail("admin panel response should be an HTML route response");
  }
  if (typeof response.body !== "string") {
    assert.fail("admin panel response should have an HTML string body");
  }
  assert.equal(response.status, 200);
  assert.equal(response.headers["cache-control"], "no-store");
  assert.equal(response.headers["content-type"], "text/html; charset=utf-8");
  assert.match(response.body, /Rusty Crew Admin/);
  assert.match(response.body, /tokenForm/);
});

test("static site route helpers keep serving rules bounded", async () => {
  assert.equal(
    staticSiteRootFromPaths({ dataDir: "/srv/rusty-crew" }),
    "/srv/rusty-crew/site",
  );
  assert.equal(
    staticContentType("bundle.js"),
    "application/javascript; charset=utf-8",
  );
  assert.equal(staticContentType("unknown.bin"), "application/octet-stream");
  assert.equal(
    staticCacheControl(
      "/srv/rusty-crew/site",
      "/srv/rusty-crew/site/index.html",
    ),
    "no-cache",
  );
  assert.equal(
    staticCacheControl(
      "/srv/rusty-crew/site",
      "/srv/rusty-crew/site/app-1234567890abcdef.js",
    ),
    "public, max-age=31536000, immutable",
  );

  assert.deepEqual(
    resolveStaticSitePath("/srv/rusty-crew/site", "/assets/app.js"),
    {
      ok: true,
      path: "/srv/rusty-crew/site/assets/app.js",
    },
  );
  assert.deepEqual(
    resolveStaticSitePath("/srv/rusty-crew/site", "/../secret.txt"),
    {
      ok: false,
      reasonCode: "static_path_forbidden",
      message: "static path contains a forbidden segment",
    },
  );

  const methodFailure = await handleStaticSiteRequest(
    {
      method: "POST",
      pathname: "/",
      requestId: "req-static",
    },
    { root: "/srv/rusty-crew/site" },
  );
  if ("kind" in methodFailure) {
    assert.fail("method failure should return a JSON route envelope");
  }
  if (typeof methodFailure.body === "string") {
    assert.fail("method failure should return a JSON route envelope");
  }
  assert.equal(methodFailure.status, 405);
  assert.equal(
    methodFailure.body.error?.reason_code,
    "static_method_not_allowed",
  );
});

test("host shell route delegates API traffic and owns browser shell paths", async () => {
  const config = {
    admin: { authMode: "none" },
    paths: { dataDir: "/srv/rusty-crew" },
  };

  const admin = await handleHostShellRequest(
    request({ url: "/admin", method: "GET" }),
    config as never,
  );
  if (admin === undefined || "kind" in admin) {
    assert.fail("admin shell route should return an HTML route response");
  }
  assert.equal(admin?.status, 200);

  const api = await handleHostShellRequest(
    request({ url: "/v1/admin/diagnostics", method: "GET" }),
    config as never,
  );
  assert.equal(api, undefined);
});

function request(input: { url: string; method?: string }): IncomingMessage {
  return {
    method: input.method,
    url: input.url,
    headers: {},
  } as IncomingMessage;
}
