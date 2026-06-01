export type WorkbenchSignal = "metrics" | "logs" | "traces";
export type WorkbenchMode = "nlq" | "raw";

export interface WorkbenchBlockState {
  id: string;
  signal: WorkbenchSignal;
  mode: WorkbenchMode;
  draft: string;
  collapsed: boolean;
}

export interface NotebookStateV1 {
  version: 1;
  title: string;
  activeBlockId: string;
  blocks: WorkbenchBlockState[];
}

const WORKBENCH_TITLE = "Query Workbench";
const DEFAULT_BLOCKS: WorkbenchSignal[] = ["metrics", "logs", "traces"];

export function createStarterWorkbenchState(): NotebookStateV1 {
  return {
    version: 1,
    title: WORKBENCH_TITLE,
    activeBlockId: DEFAULT_BLOCKS[0],
    blocks: DEFAULT_BLOCKS.map((signal) => createBlockState(signal)),
  };
}

export function encodeWorkbenchState(state: NotebookStateV1): string {
  return toBase64Url(JSON.stringify(normalizeWorkbenchState(state)));
}

export function decodeWorkbenchState(stateParam?: string | null): NotebookStateV1 {
  if (!stateParam) return createStarterWorkbenchState();

  try {
    const json = fromBase64Url(stateParam);
    const parsed = JSON.parse(json) as unknown;
    return normalizeDecodedState(parsed);
  } catch {
    return createStarterWorkbenchState();
  }
}

export function normalizeWorkbenchState(state: NotebookStateV1): NotebookStateV1 {
  return {
    version: 1,
    title: state.title.trim() || WORKBENCH_TITLE,
    activeBlockId: state.activeBlockId || DEFAULT_BLOCKS[0],
    blocks: state.blocks.map((block) => normalizeBlockState(block)),
  };
}

function normalizeDecodedState(value: unknown): NotebookStateV1 {
  if (!isPlainObject(value)) return createStarterWorkbenchState();
  if (value.version !== 1) return createStarterWorkbenchState();
  if (typeof value.title !== "string") return createStarterWorkbenchState();
  if (typeof value.activeBlockId !== "string") return createStarterWorkbenchState();
  if (!Array.isArray(value.blocks) || value.blocks.length !== DEFAULT_BLOCKS.length) {
    return createStarterWorkbenchState();
  }

  const blocks = value.blocks.map((block) => {
    if (!isPlainObject(block)) return null;
    if (typeof block.id !== "string") return null;
    if (!isWorkbenchSignal(block.signal)) return null;
    if (!isWorkbenchMode(block.mode)) return null;
    if (typeof block.draft !== "string") return null;
    if (typeof block.collapsed !== "boolean") return null;
    return normalizeBlockState({
      id: block.id,
      signal: block.signal,
      mode: block.mode,
      draft: block.draft,
      collapsed: block.collapsed,
    });
  });

  if (blocks.some((block) => block === null)) return createStarterWorkbenchState();

  return {
    version: 1,
    title: value.title.trim() || WORKBENCH_TITLE,
    activeBlockId: value.activeBlockId,
    blocks: blocks as WorkbenchBlockState[],
  };
}

function createBlockState(signal: WorkbenchSignal): WorkbenchBlockState {
  return {
    id: signal,
    signal,
    mode: "nlq",
    draft: "",
    collapsed: false,
  };
}

function normalizeBlockState(block: Pick<WorkbenchBlockState, "id" | "signal" | "mode" | "draft" | "collapsed">): WorkbenchBlockState {
  return {
    id: block.id,
    signal: block.signal,
    mode: block.mode,
    draft: block.draft,
    collapsed: block.collapsed,
  };
}

function isWorkbenchSignal(value: unknown): value is WorkbenchSignal {
  return value === "metrics" || value === "logs" || value === "traces";
}

function isWorkbenchMode(value: unknown): value is WorkbenchMode {
  return value === "nlq" || value === "raw";
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toBase64Url(value: string): string {
  const base64 = toBase64(value);
  return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

function fromBase64Url(value: string): string {
  const padded = value.replace(/-/g, "+").replace(/_/g, "/");
  const padLength = (4 - (padded.length % 4)) % 4;
  return fromBase64(padded + "=".repeat(padLength));
}

function toBase64(value: string): string {
  const buffer = (globalThis as typeof globalThis & {
    Buffer?: { from(input: string, encoding: string): { toString(encoding: string): string } };
  }).Buffer;
  if (buffer) {
    return buffer.from(value, "utf8").toString("base64");
  }

  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function fromBase64(value: string): string {
  const buffer = (globalThis as typeof globalThis & {
    Buffer?: { from(input: string, encoding: string): { toString(encoding: string): string } };
  }).Buffer;
  if (buffer) {
    return buffer.from(value, "base64").toString("utf8");
  }

  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}
