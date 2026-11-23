import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

export type ThemePreference = "light" | "dark" | "system";
export type ContrastPreference = "default" | "more";
type ResolvedTheme = "light" | "dark";

interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
  contrastPreference: ContrastPreference;
  setContrastPreference: (contrastPreference: ContrastPreference) => void;
}

export const THEME_STORAGE_KEY = "observable.theme";
export const CONTRAST_STORAGE_KEY = "observable.contrast";

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => getStoredThemePreference());
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(() => getSystemTheme());
  const [contrastPreference, setContrastPreferenceState] = useState<ContrastPreference>(() =>
    getStoredContrastPreference()
  );

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => setSystemTheme(media.matches ? "dark" : "light");

    handleChange();
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  const resolvedTheme = preference === "system" ? systemTheme : preference;

  useEffect(() => {
    document.documentElement.dataset.colorScheme = resolvedTheme;
    document.documentElement.dataset.themePreference = preference;
    document.documentElement.dataset.contrast = contrastPreference;
    document.documentElement.dataset.density = "compact";
  }, [preference, resolvedTheme, contrastPreference]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      preference,
      resolvedTheme,
      setPreference: (nextPreference) => {
        window.localStorage.setItem(THEME_STORAGE_KEY, nextPreference);
        setPreferenceState(nextPreference);
      },
      contrastPreference,
      setContrastPreference: (nextContrastPreference) => {
        window.localStorage.setItem(CONTRAST_STORAGE_KEY, nextContrastPreference);
        setContrastPreferenceState(nextContrastPreference);
      },
    }),
    [preference, resolvedTheme, contrastPreference]
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) {
    throw new Error("useTheme must be used inside ThemeProvider");
  }
  return value;
}

export function getStoredThemePreference(): ThemePreference {
  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return isThemePreference(stored) ? stored : "system";
}

export function getStoredContrastPreference(): ContrastPreference {
  const stored = window.localStorage.getItem(CONTRAST_STORAGE_KEY);
  return isContrastPreference(stored) ? stored : "default";
}

function getSystemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function isThemePreference(value: string | null): value is ThemePreference {
  return value === "light" || value === "dark" || value === "system";
}

function isContrastPreference(value: string | null): value is ContrastPreference {
  return value === "default" || value === "more";
}
