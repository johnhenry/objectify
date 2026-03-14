export interface ObjectRow {
  id: string;
  class: string | null;
  description: string | null;
  schema: string | null;
  createdAt: string;
  expiresAt: string | null;
}

export interface LogEntry {
  version: number;
  method: string;
  at: string;
}

export interface InspectResult {
  id: string;
  shortId: string;
  class: string | null;
  description: string | null;
  versions: number;
  createdAt: string;
  expiresAt: string | null;
}

export interface ListOptions {
  class?: string;
  expired?: boolean;
  since?: string;
  limit?: number;
  offset?: number;
}

export interface CreateOptions {
  description?: string;
  class?: string;
  expire?: string;
}

export interface ClassResult {
  result: unknown;
  stateChanged: boolean;
  state: unknown | null;
}

export interface ObjectifyOptions {
  dir?: string;
}
