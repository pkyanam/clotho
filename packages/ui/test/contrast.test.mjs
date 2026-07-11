import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const css = readFileSync(
  new URL("../styles/tokens.css", import.meta.url),
  "utf8",
);

function declarations(selector) {
  const match = css.match(new RegExp(`${selector}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `missing token block: ${selector}`);
  return Object.fromEntries(
    [...match[1].matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/gi)].map((entry) => [
      entry[1],
      entry[2],
    ]),
  );
}

function luminance(hex) {
  const channels = hex
    .slice(1)
    .match(/../g)
    .map((value) => Number.parseInt(value, 16) / 255)
    .map((value) =>
      value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
    );
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(a, b) {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter + 0.05) / (darker + 0.05);
}

const themes = {
  dark: declarations(':root,\\s*:root\\[data-mode="dark"\\]'),
  light: declarations(':root\\[data-mode="light"\\]'),
};

for (const [name, theme] of Object.entries(themes)) {
  test(`${name} meaningful text meets WCAG AA`, () => {
    for (const role of ["foreground", "muted", "faint"]) {
      assert.ok(
        contrast(theme[role], theme.background) >= 4.5,
        `${role} on background is below 4.5:1`,
      );
      assert.ok(
        contrast(theme[role], theme.surface) >= 4.5,
        `${role} on surface is below 4.5:1`,
      );
    }
  });

  test(`${name} controls and focus meet WCAG non-text contrast`, () => {
    for (const role of ["border", "focus"]) {
      assert.ok(
        contrast(theme[role], theme.background) >= 3,
        `${role} on background is below 3:1`,
      );
      assert.ok(
        contrast(theme[role], theme.surface) >= 3,
        `${role} on surface is below 3:1`,
      );
    }
  });

  test(`${name} primary action text meets WCAG AA`, () => {
    assert.ok(
      contrast(theme.action, "#ffffff") >= 4.5,
      "white primary-action text is below 4.5:1",
    );
  });
}
