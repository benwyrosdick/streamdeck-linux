// Run with: node --test tests/
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

// Deck.js is a QML JavaScript library (.pragma library); load it as a script.
const source = readFileSync(new URL("../Deck.js", import.meta.url), "utf8").replace(".pragma library", "");
const Deck = {};
vm.runInNewContext(source, Deck);
// Objects made inside the vm context have that context's prototypes; compare plain copies.
const plain = (v) => JSON.parse(JSON.stringify(v));

test("sanitizes device ids the way the service names frame folders", () => {
  assert.equal(Deck.sanitizeId("A00SA4092MJEJI"), "A00SA4092MJEJI");
  assert.equal(Deck.sanitizeId("../../etc"), "______etc");
  assert.equal(Deck.framePath("/run/user/1000/streamdeck-linux", "virtual-1", "key", 3), "/run/user/1000/streamdeck-linux/frames/virtual-1/key-3.png");
});

test("bounds untrusted state", () => {
  const d = Deck.device({ id: "x", name: "N".repeat(500), columns: 9999, rows: -4, brightness: "70", virtual: true });
  assert.equal(d.name.length, 64);
  assert.equal(d.columns, 16);
  assert.equal(d.rows, 1);
  assert.equal(d.brightness, 70);
  assert.equal(d.isVirtual, true);
  assert.equal(Deck.devices({ devices: "nope" }).length, 0);
  assert.equal(Deck.devices(null).length, 0);
});

test("prefers the chosen device, then hardware", () => {
  const list = Deck.devices({ devices: [{ id: "virtual-1", virtual: true }, { id: "HW" }] });
  assert.equal(Deck.pick(list, "").id, "HW");
  assert.equal(Deck.pick(list, "virtual-1").id, "virtual-1");
  assert.equal(Deck.pick(list, "gone").id, "HW");
  assert.equal(Deck.pick([], "x"), null);
});

test("key positions are column,row", () => {
  assert.equal(Deck.keyPosition(0, 5), "0,0");
  assert.equal(Deck.keyPosition(7, 5), "2,1");
  assert.equal(Deck.keyPosition(14, 5), "4,2");
});

test("parses replies and events, rejects junk", () => {
  assert.deepEqual(plain(Deck.parseLine('{"id":3,"result":true}')), { kind: "reply", id: 3, result: true, error: "" });
  const e = Deck.parseLine('{"event":"state","data":{"devices":[]}}');
  assert.equal(e.kind, "event");
  assert.equal(e.name, "state");
  assert.equal(Deck.parseLine("not json").kind, "bad");
  assert.equal(Deck.parseLine("42").kind, "bad");
  assert.equal(Deck.parseLine('{"id":1,"error":{"x":1}}').error, "");
});

test("markup in names stays text and is shortened", () => {
  const opts = Deck.profileOptions([{ id: "a", name: '<img src="https://example.invalid/x">' }, { id: 5 }, null]);
  assert.equal(opts.length, 1);
  assert.equal(opts[0].label, '<img src="https://example.invalid/x">');
  assert.equal(Deck.barLabel({ profileName: "A very long profile name" }, 10), "A very lo…");
});

test("request lines are newline-terminated JSON", () => {
  const line = Deck.requestLine(7, "press", { device: "d", position: "1,0" });
  assert.ok(line.endsWith("\n"));
  assert.deepEqual(JSON.parse(line), { id: 7, method: "press", params: { device: "d", position: "1,0" } });
});
