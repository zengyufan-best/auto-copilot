import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { PipelineEditor } from "./components/PipelineEditor";
import { ControlBar } from "./components/ControlBar";
import type { Action, Pipeline, ExecutionStatus, RecordedEvent } from "./types";

const MIN_GAP_MS = 50;

function App() {
  const [pipeline, setPipeline] = useState<Pipeline>({
    name: "新流水线",
    actions: [],
    loopCount: 1,
  });

  const [status, setStatus] = useState<ExecutionStatus>({
    running: false,
    currentLoop: 0,
    currentAction: 0,
  });

  const [recording, setRecording] = useState(false);

  useEffect(() => {
    const unlistenStatus = listen<ExecutionStatus>("pipeline-status", (event) => {
      setStatus(event.payload);
    });

    const unlistenRecord = listen<RecordedEvent>("recorded-action", (event) => {
      const { action, gapMs } = event.payload;
      setPipeline((prev) => {
        const actions = [...prev.actions];
        if (actions.length > 0 && gapMs >= MIN_GAP_MS) {
          actions.push({ type: "delay", ms: Math.round(gapMs) });
        }
        actions.push(action);
        return { ...prev, actions };
      });
    });

    const unlistenError = listen<string>("recording-error", (event) => {
      setRecording(false);
      alert(event.payload);
    });

    return () => {
      unlistenStatus.then((fn) => fn());
      unlistenRecord.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleAddAction = useCallback((action: Action) => {
    setPipeline((prev) => ({
      ...prev,
      actions: [...prev.actions, action],
    }));
  }, []);

  const handleRemoveAction = useCallback((index: number) => {
    setPipeline((prev) => ({
      ...prev,
      actions: prev.actions.filter((_, i) => i !== index),
    }));
  }, []);

  const handleUpdateAction = useCallback((index: number, action: Action) => {
    setPipeline((prev) => ({
      ...prev,
      actions: prev.actions.map((a, i) => (i === index ? action : a)),
    }));
  }, []);

  const handleMoveAction = useCallback(
    (fromIndex: number, toIndex: number) => {
      setPipeline((prev) => {
        const actions = [...prev.actions];
        const [moved] = actions.splice(fromIndex, 1);
        actions.splice(toIndex, 0, moved);
        return { ...prev, actions };
      });
    },
    []
  );

  const handleSetLoopCount = useCallback((count: number) => {
    setPipeline((prev) => ({ ...prev, loopCount: count }));
  }, []);

  const handleStart = useCallback(async () => {
    try {
      await invoke("run_pipeline", { pipeline });
    } catch (e) {
      console.error("Failed to start pipeline:", e);
    }
  }, [pipeline]);

  const handleStop = useCallback(async () => {
    try {
      await invoke("stop_pipeline");
    } catch (e) {
      console.error("Failed to stop pipeline:", e);
    }
  }, []);

  const handleToggleRecording = useCallback(async () => {
    try {
      if (recording) {
        await invoke("stop_recording");
        setRecording(false);
      } else {
        await invoke("start_recording");
        setRecording(true);
      }
    } catch (e) {
      console.error("Failed to toggle recording:", e);
    }
  }, [recording]);

  const handleGetCursorPosition = useCallback(async () => {
    try {
      const pos = await invoke<{ x: number; y: number }>("get_cursor_position");
      return pos;
    } catch (e) {
      console.error("Failed to get cursor position:", e);
      return null;
    }
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1>Auto-Pilot</h1>
        <span className="subtitle">自动化流水线</span>
      </header>

      <main className="app-main">
        <PipelineEditor
          actions={pipeline.actions}
          onAdd={handleAddAction}
          onRemove={handleRemoveAction}
          onMove={handleMoveAction}
          onUpdate={handleUpdateAction}
          onGetCursorPosition={handleGetCursorPosition}
          recording={recording}
          onToggleRecording={handleToggleRecording}
        />
      </main>

      <footer className="app-footer">
        <ControlBar
          loopCount={pipeline.loopCount}
          onSetLoopCount={handleSetLoopCount}
          running={status.running}
          currentLoop={status.currentLoop}
          currentAction={status.currentAction}
          totalActions={pipeline.actions.length}
          onStart={handleStart}
          onStop={handleStop}
        />
      </footer>
    </div>
  );
}

export default App;
