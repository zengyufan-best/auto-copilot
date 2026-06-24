import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Action, MouseButton } from "../types";

interface Props {
  onSubmit: (action: Action) => void;
  onCancel: () => void;
}

type ActionType = Action["type"];
type Picking = null | "position" | "key";

export function ActionForm({ onSubmit, onCancel }: Props) {
  const [actionType, setActionType] = useState<ActionType>("mouse_click");
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const [button, setButton] = useState<MouseButton>("left");
  const [key, setKey] = useState("");
  const [modifiers, setModifiers] = useState<string[]>([]);
  const [text, setText] = useState("");
  const [ms, setMs] = useState(1000);
  const [picking, setPicking] = useState<Picking>(null);

  useEffect(() => {
    const unPos = listen<{ x: number; y: number }>("picked-position", (e) => {
      setX(e.payload.x);
      setY(e.payload.y);
      setPicking(null);
    });
    const unKey = listen<{ key: string; modifiers: string[] }>(
      "picked-key",
      (e) => {
        setKey(e.payload.key);
        setModifiers(e.payload.modifiers ?? []);
        setPicking(null);
      }
    );
    return () => {
      unPos.then((f) => f());
      unKey.then((f) => f());
      // Make sure no pick stays armed after the form closes.
      invoke("cancel_pick").catch(() => {});
    };
  }, []);

  const handlePickPosition = async () => {
    setPicking("position");
    try {
      await invoke("start_pick_position");
    } catch (err) {
      setPicking(null);
      alert(String(err));
    }
  };

  const handlePickKey = async () => {
    setPicking("key");
    try {
      await invoke("start_pick_key");
    } catch (err) {
      setPicking(null);
      alert(String(err));
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
                className={`btn ${picking === "position" ? "btn-picking" : "btn-secondary"}`}
                onClick={handlePickPosition}
                disabled={picking !== null}
              >
                {picking === "position" ? "请点击屏幕…" : "拾取坐标"}
              </button>
            </div>
            {picking === "position" && (
              <div className="pick-hint">
                正在拾取坐标:把鼠标移到目标位置点击一下,坐标会自动填入。
              </div>
            )}
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
              <div className="form-row">
                <input
                  type="text"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  placeholder="例如: a, Enter, F5, Space"
                />
                <button
                  className={`btn ${picking === "key" ? "btn-picking" : "btn-secondary"}`}
                  onClick={handlePickKey}
                  disabled={picking !== null}
                >
                  {picking === "key" ? "请按键…" : "拾取键位"}
                </button>
              </div>
            </div>
            {picking === "key" && (
              <div className="pick-hint">
                正在拾取键位:按下任意按键(可带 Ctrl/Shift/Alt),会自动填入。
              </div>
            )}
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
