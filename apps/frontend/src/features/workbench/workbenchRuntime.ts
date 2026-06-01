import type { NlqResponse } from "../../api/nlq";

export type WorkbenchQueryState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "result"; response: NlqResponse; question: string };

export function createIdleWorkbenchQueryState(): WorkbenchQueryState {
  return { status: "idle" };
}

export function createIdleWorkbenchQueryStateMap(blockIds: string[]): Record<string, WorkbenchQueryState> {
  return Object.fromEntries(blockIds.map((id) => [id, createIdleWorkbenchQueryState()]));
}
