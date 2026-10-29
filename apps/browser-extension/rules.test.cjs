const { test } = require("node:test");
const assert = require("node:assert/strict");
const rules = require("./rules.js");
test("closed Sheets commands never forward labels or arbitrary fields", () => {
  assert.equal(rules.action("Import…"), "import");
  assert.equal(rules.action("ダウンロード"), "export");
  for (const label of [
    "Customer secret",
    "Share Customer secret",
    "x".repeat(81),
    null,
  ])
    assert.equal(rules.action(label), null);
  assert.ok(rules.observation({ kind: "observation", action: "export" }));
  assert.ok(
    !rules.observation({
      kind: "observation",
      action: "export",
      title: "secret",
    }),
  );
  assert.ok(!rules.observation({ kind: "observation", action: "unknown" }));
});
test("only positive active top-level nonprivate Sheets context authorizes", () => {
  const tab = {
    id: 1,
    windowId: 2,
    active: true,
    incognito: false,
    url: "https://docs.google.com/spreadsheets/d/synthetic/edit",
  };
  const sender = {
    id: "our-id",
    frameId: 0,
    origin: "https://docs.google.com",
    tab,
    url: tab.url,
  };
  const window = { id: 2, focused: true };
  assert.ok(rules.allowedSender(sender, tab, window, "our-id"));
  for (const bad of [
    { ...sender, frameId: 1 },
    { ...sender, id: "other" },
    { ...sender, origin: "null" },
    { ...sender, tab: { ...tab, incognito: true } },
    { ...sender, url: "https://docs.google.com/document/d/secret" },
  ])
    assert.ok(!rules.allowedSender(bad, tab, window, "our-id"));
  for (const bad of [
    { ...tab, incognito: true },
    { ...tab, active: false },
    { ...tab, incognito: undefined },
    { ...tab, id: 3 },
    { ...tab, url: "https://docs.google.com.evil.test/spreadsheets/" },
  ])
    assert.ok(!rules.allowedSender(sender, bad, window, "our-id"));
  assert.ok(
    !rules.allowedSender(sender, tab, { ...window, focused: false }, "our-id"),
  );
  for (const url of [
    "http://docs.google.com/spreadsheets/",
    "https://user@docs.google.com/spreadsheets/",
    "https://docs.google.com:8443/spreadsheets/",
    "https://docs.google.com/spreadsheets",
  ])
    assert.ok(!rules.sheetsUrl(url));
});
