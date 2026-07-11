"use client";

import { useTheme } from "src/components/theme-provider";
import { THEME_PREFERENCES, type ThemePreference } from "src/lib/theme";

const LABELS: Record<ThemePreference, { title: string; body: string }> = {
  system: {
    title: "system",
    body: "match the operating system light or dark setting.",
  },
  dark: {
    title: "dark",
    body: "deep ink canvas with raised navy work surfaces.",
  },
  light: {
    title: "light",
    body: "soft slate canvas with crisp white work surfaces.",
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
                className={`relative rounded-lg border px-4 py-3 text-left transition-colors ${
                  selected
                    ? "border-accent bg-accent-surface text-kumo-default"
                    : "border-kumo-hairline bg-kumo-base text-kumo-inactive hover:border-accent hover:text-kumo-default"
                }`}
              >
                <span className="block text-[0.9375rem]">{meta.title}</span>
                <span className="mt-1.5 block text-[0.8125rem] leading-relaxed">
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
