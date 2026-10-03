// Offline checks for answer validation, site naming and the URL guard (ported from jev-ultrafast's tests).
import assert from "node:assert/strict";
import { namedSite, validateChoice } from "./lib/site.ts";

const ok = { choice: "a", confidence: 0.8, probabilities: { a: 0.9, b: 0.1 } };
assert.equal(validateChoice(ok, ["a", "b"]).choice, "a");
for (const bad of [
  { ...ok, choice: "c" },
  { ...ok, probabilities: { a: 0.9 } },
  { ...ok, probabilities: { a: 0.6, b: 0.6 } },
  { ...ok, choice: "b" },
  { ...ok, confidence: 2 },
]) assert.throws(() => validateChoice(bad, ["a", "b"]), /no action executed/);

assert.equal(namedSite("Who wrote The Pragmatic Programmer? Use Wikipedia"), "https://en.wikipedia.org/");
assert.equal(namedSite("On MDN, find what Array.prototype.at() returns"), "https://developer.mozilla.org/");
assert.equal(namedSite("What is the latest LTS version of Node.js?"), undefined);

// OPEN_URL stays on the current site or a site the goal names.
{
  const { allowedUrl } = await import("./lib/site.ts");
  const yt = "https://www.youtube.com/watch?v=abc";
  assert.equal(allowedUrl("https://www.youtube.com/watch?v=abc&t=600s", yt, "jump to 10:00"), true);
  assert.equal(allowedUrl("https://evil.example/phish", yt, "jump to 10:00"), false);
  assert.equal(allowedUrl("https://github.com/anthropics/claude-code", "https://search.brave.com/search?q=x", "stars of claude-code on GitHub"), true);
  assert.equal(allowedUrl("https://nodejs.org/en", "https://search.brave.com/search?q=x", "latest LTS on nodejs.org"), true);
  assert.equal(allowedUrl("javascript:alert(1)", yt, "x"), false);
}
