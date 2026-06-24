import { useState } from "react";
import type { Action, MouseButton } from "../types";

interface Props {
  onSubmit: (action: Action) => void;
  onCancel: () => void;
  onGetCursorPosition: () => Promise<{ x: number; y: number } | null>;
}

type ActionType = Action["type"];

export function ActionForm({ onSubmit, onCancel, onGetCursorPosition }: Props) {
  const [actionType, setActionType] = useState<ActionType>("mouse_click");
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const [button, setButton] = useState<MouseButton>("left");
  const [key, setKey] = useState("");
  const [modifiers, setModifiers] = useState<string[]>([]);
  const [text, setText] = useState("");
  const [ms, setMs] = useState(1000);

  const handlePickPosition = async () => {
    const pos = await onGetCursorPosition();
    if (pos) {
      setX(pos.x);
      setY(pos.y);
    }
  };

  const handleSubmit = () => {
    switch (actionType) {
      case "mouse_click":
        onSubmit({ type: "mouse_click", x, y, button });
        break;
      case "mouse_move":
        onSubmit({ type: "mouse_move", x, y });
        break;
      case "key_press":
        onSubmit({ type: "key_press", key, modifiers });
        break;
      case "key_type":
        onSubmit({ type: "key_type", text });
        break;
      case "delay":
        onSubmit({ type: "delay", ms });
        break;
    }
  };

  const toggleModifier = (mod: string) => {
    setModifiers((prev) =>
      prev.includes(mod) ? prev.filter((m) => m !== mod) : [...prev, mod]
    );
  };

  return (
    <div className="modal-overlay">
      <div className="action-form">
        <h3>添加操作</h3>

        <div className="form-group">
          <label>操作类型</label>
          <select
            value={actionType}
            onChange={(e) => setActionType(e.target.value as ActionType)}
          >
            <option value="mouse_click">鼠标点击</option>
            <option value="mouse_move">鼠标移动</option>
            <option value="key_press">按键</option>
            <option value="key_type">输入文本</option>
            <option value="delay">延迟等待</option>
          </select>
        </div>

        {(actionType === "mouse_click" || actionType === "mouse_move") && (
          <>
            <div className="form-row">
              <div className="form-group">
                <label>X 坐标</label>
                <input
                  type="number"
                  value={x}
                  onChange={(e) => setX(Number(e.target.value))}
                />
              </div>
              <div className="form-group">
                <label>Y 坐标</label>
                <input
                  type="number"
                  value={y}
                  onChange={(e) => setY(Number(e.target.value))}
                />
              </div>
              <button
                className="btn btn-secondary"
                onClick={handlePickPosition}
                title="3秒后获取鼠标位置"
              >
                拾取坐标
              </button>
            </div>
            {actionType === "mouse_click" && (
              <div className="form-group">
                <label>鼠标按键</label>
                <select
                  value={button}
                  onChange={(e) => setButton(e.target.value as MouseButton)}
                >
                  <option value="left">左键</option>
                  <option value="right">右键</option>
                  <option value="middle">中键</option>
                </select>
              </div>
            )}
          </>
        )}

        {actionType === "key_press" && (
          <>
            <div className="form-group">
              <label>按键</label>
              <input
                type="text"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder="例如: a, Enter, F5, Space"
              />
            </div>
            <div className="form-group">
              <label>修饰键</label>
              <div className="modifier-buttons">
                {["Ctrl", "Shift", "Alt", "Meta"].map((mod) => (
                  <button
                    key={mod}
                    className={`btn btn-modifier ${modifiers.includes(mod) ? "active" : ""}`}
                    onClick={() => toggleModifier(mod)}
                  >
                    {mod}
                  </button>
                ))}
              </div>
            </div>
          </>
        )}

        {actionType === "key_type" && (
          <div className="form-group">
            <label>输入文本</label>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="要输入的文本内容"
              rows={3}
            />
          </div>
        )}

        {actionType === "delay" && (
          <div className="form-group">
            <label>延迟时间 (毫秒)</label>
            <input
              type="number"
              value={ms}
              onChange={(e) => setMs(Number(e.target.value))}
              min={0}
              step={100}
            />
            <span className="hint">{(ms / 1000).toFixed(1)} 秒</span>
          </div>
        )}

        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onCancel}>
            取消
          </button>
          <button className="btn btn-primary" onClick={handleSubmit}>
            确认添加
          </button>
        </div>
      </div>
    </div>
  );
}
