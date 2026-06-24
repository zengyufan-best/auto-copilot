export type MouseButton = "left" | "right" | "middle";

export type Action =
  | { type: "mouse_click"; x: number; y: number; button: MouseButton }
  | { type: "mouse_move"; x: number; y: number }
  | { type: "key_press"; key: string; modifiers: string[] }
  | { type: "key_type"; text: string }
  | { type: "delay"; ms: number };

export interface Pipeline {
  name: string;
  actions: Action[];
  loopCount: number; // 0 = infinite
}

export interface ExecutionStatus {
  running: boolean;
  currentLoop: number;
  currentAction: number;
}

export interface RecordedEvent {
  action: Action;
  gapMs: number;
}
