interface Props {
  loopCount: number;
  onSetLoopCount: (count: number) => void;
  running: boolean;
  currentLoop: number;
  currentAction: number;
  totalActions: number;
  onStart: () => void;
  onStop: () => void;
}

export function ControlBar({
  loopCount,
  onSetLoopCount,
  running,
  currentLoop,
  currentAction,
  totalActions,
  onStart,
  onStop,
}: Props) {
  return (
    <div className="control-bar">
      <div className="control-left">
        <div className="form-group inline">
          <label>循环次数</label>
          <input
            type="number"
            value={loopCount}
            onChange={(e) => onSetLoopCount(Number(e.target.value))}
            min={0}
            disabled={running}
          />
          <span className="hint">{loopCount === 0 ? "无限循环" : `${loopCount} 次`}</span>
        </div>
      </div>

      <div className="control-center">
        {running && (
          <span className="status-text">
            执行中: 第 {currentLoop + 1} 轮, 步骤 {currentAction + 1}/{totalActions}
          </span>
        )}
      </div>

      <div className="control-right">
        {!running ? (
          <button
            className="btn btn-start"
            onClick={onStart}
            disabled={totalActions === 0}
          >
            ▶ 开始执行
          </button>
        ) : (
          <button className="btn btn-stop" onClick={onStop}>
            ■ 停止
          </button>
        )}
      </div>
    </div>
  );
}
