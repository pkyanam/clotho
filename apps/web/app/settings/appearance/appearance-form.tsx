"use client";

import { useTheme } from "src/components/theme-provider";
import {
  THEME_PREFERENCES,
  type ThemePreference,
} from "src/lib/theme";

const LABELS: Record<
  ThemePreference,
  { title: string; body: string }
> = {
  system: {
    title: "system",
    body: "match the operating system light or dark setting.",
  },
  dark: {
    title: "dark",
    body: "pure black canvas — the default clotho console.",
  },
  light: {
    title: "light",
    body: "inverted belweave: white canvas, black type and hairlines.",
  },
};

export function AppearanceForm() {
  const { preference, resolved, setPreference } = useTheme();

  return (
    <div>
      <fieldset>
        <legend className="sr-only">color scheme</legend>
        <div
          className="grid gap-2 sm:grid-cols-3"
          role="radiogroup"
          aria-label="color scheme"
        >
          {THEME_PREFERENCES.map((option) => {
            const selected = preference === option;
            const meta = LABELS[option];
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setPreference(option)}
                className={`border px-4 py-3 text-left transition-colors ${
                  selected
                    ? "border-kumo-contrast bg-kumo-elevated text-kumo-default"
                    : "border-kumo-hairline text-kumo-inactive hover:border-kumo-contrast hover:text-kumo-default"
                }`}
              >
                <span className="block text-[0.9375rem]">{meta.title}</span>
                <span className="mt-1.5 block text-[0.8125rem] leading-relaxed opacity-80">
                  {meta.body}
                </span>
              </button>
            );
          })}
        </div>
      </fieldset>
      <p className="mt-4 text-[0.8125rem] text-kumo-inactive">
        active: {resolved}
        {preference === "system" ? " (from system)" : ""}
      </p>
    </div>
  );
}
