// apps/frontend/src/features/onboarding/onboardingState.ts

const STORAGE_KEY = "observable_onboarding";

export type WizardStep = "language" | "apikey" | "waiting" | "done";

export type Language =
  | "nodejs"
  | "python"
  | "java"
  | "go"
  | "ruby"
  | "dotnet"
  | "other";

interface OnboardingState {
  step: WizardStep;
  language: Language | null;
  tokenId: string | null;
  complete: boolean;
}

const DEFAULT_STATE: OnboardingState = {
  step: "language",
  language: null,
  tokenId: null,
  complete: false,
};

export function readOnboardingState(): OnboardingState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_STATE };
    return { ...DEFAULT_STATE, ...JSON.parse(raw) } as OnboardingState;
  } catch {
    return { ...DEFAULT_STATE };
  }
}

export function writeOnboardingState(patch: Partial<OnboardingState>): void {
  const current = readOnboardingState();
  localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...current, ...patch }));
}

export function clearOnboardingState(): void {
  localStorage.removeItem(STORAGE_KEY);
}

export function isOnboardingComplete(): boolean {
  return readOnboardingState().complete;
}
