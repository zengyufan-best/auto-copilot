import type { Action } from "../types";

interface Props {
  index: number;
  action: Action;
  total: number;
  onRemove: () => void;
  onUpdate: (action: Action) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}

const ACTION_LABELS: Record<Action["type"], string> = {
  mouse_click: "🖱 鼠标点击",
  mouse_move: "↗ 鼠标移动",
  key_press: "⌨ 按键",
  key_type: "📝 输入文本",
  delay: "⏱ 延迟",
};

function describeAction(action: Action): string {
  switch (action.type) {
    case "mouse_click":
      return `(${action.x}, ${action.y}) ${action.button}键`;
    case "mouse_move":
      return `移动到 (${action.x}, ${action.y})`;
    case "key_press": {
      const mods = action.modifiers.length
        ? action.modifiers.join("+") + "+"
        : "";
      return `${mods}${action.key}`;
    }
    case "key_type":
      return `"${action.text.length > 20 ? action.text.slice(0, 20) + "..." : action.text}"`;
    case "delay":
      return `${action.ms} ms`;
  }
}

export function ActionItem({
  index,
  action,
  total,
  onRemove,
  onUpdate,
  onMoveUp,
  onMoveDown,
}: Props) {
  return (
    <div className="action-item">
      <span className="action-index">{index + 1}</span>
      <div className="action-info">
        <span className="action-label">{ACTION_LABELS[action.type]}</span>
        {action.type === "delay" ? (
          <span className="action-desc action-desc-edit">
            间隔
            <input
              type="number"
              className="inline-input"
              value={action.ms}
              min={0}
              step={100}
              onChange={(e) =>
                onUpdate({ type: "delay", ms: Number(e.target.value) })
              }
            />
            ms
          </span>
        ) : (
          <span className="action-desc">{describeAction(action)}</span>
        )}
      </div>
      <div className="action-controls">
        <button
          className="btn-icon"
          onClick={onMoveUp}
          disabled={index === 0}
          title="上移"
        >
          ▲
        </button>
        <button
          className="btn-icon"
          onClick={onMoveDown}
          disabled={index === total - 1}
          title="下移"
        >
          ▼
        </button>
        <button className="btn-icon btn-danger" onClick={onRemove} title="删除">
          ✕
        </button>
      </div>
    </div>
  );
}
