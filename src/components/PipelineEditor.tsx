import { useState } from "react";
import type { Action } from "../types";
import { ActionItem } from "./ActionItem";
import { ActionForm } from "./ActionForm";

interface Props {
  actions: Action[];
  onAdd: (action: Action) => void;
  onRemove: (index: number) => void;
  onMove: (from: number, to: number) => void;
  onUpdate: (index: number, action: Action) => void;
  onGetCursorPosition: () => Promise<{ x: number; y: number } | null>;
  recording: boolean;
  onToggleRecording: () => void;
}

export function PipelineEditor({
  actions,
  onAdd,
  onRemove,
  onMove,
  onUpdate,
  onGetCursorPosition,
  recording,
  onToggleRecording,
}: Props) {
  const [showForm, setShowForm] = useState(false);

  return (
    <div className="pipeline-editor">
      <div className="pipeline-header">
        <h2>操作列表</h2>
        <div className="pipeline-header-actions">
          <button
            className={`btn ${recording ? "btn-recording" : "btn-record"}`}
            onClick={(e) => {
              e.currentTarget.blur();
              onToggleRecording();
            }}
          >
            {recording ? "⏹ 停止录制" : "● 录制操作"}
          </button>
          <button
            className="btn btn-primary"
            onClick={() => setShowForm(true)}
            disabled={recording}
          >
            + 添加操作
          </button>
        </div>
      </div>

      {recording && (
        <div className="recording-hint">
          正在录制：在任意位置点击鼠标或按下键盘，操作会被自动记录（含间隔）
        </div>
      )}

      {actions.length === 0 ? (
        <div className="empty-state">
          <p>还没有任何操作，点击上方按钮添加</p>
        </div>
      ) : (
        <div className="action-list">
          {actions.map((action, index) => (
            <ActionItem
              key={index}
              index={index}
              action={action}
              total={actions.length}
              onRemove={() => onRemove(index)}
              onUpdate={(a) => onUpdate(index, a)}
              onMoveUp={() => index > 0 && onMove(index, index - 1)}
              onMoveDown={() =>
                index < actions.length - 1 && onMove(index, index + 1)
              }
            />
          ))}
        </div>
      )}

      {showForm && (
        <ActionForm
          onSubmit={(action) => {
            onAdd(action);
            setShowForm(false);
          }}
          onCancel={() => setShowForm(false)}
          onGetCursorPosition={onGetCursorPosition}
        />
      )}
    </div>
  );
}
