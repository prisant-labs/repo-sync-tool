// THROWAWAY E-12 TRACER UI - DELETE WHEN REAL SCREENS LAND
//
// This component exists only to prove the tracer round trip end to end:
// repo_add_path -> RepoId -> repo_check_now -> CheckResult + a
// repo:check-completed event. It pre-commits NO UI/UX decision and must be
// deleted when the real screens (E-06 consumers and beyond) arrive.
//
// It talks to the backend ONLY through the generated tauri-specta bindings
// (commands.* / events.*), never raw invoke/listen, so a contract change in the
// Rust source breaks this file's typecheck - which is the seam guarantee.

import { useEffect, useState } from "react";
import {
  commands,
  events,
  type CheckCompletedPayload,
  type RepoId,
} from "@/lib/bindings";

// Disposable seed path. Edit to a real local git repo on your machine when
// exercising the tracer. Throwaway, like the rest of this file.
const PLACEHOLDER_REPO_PATH = "C:\\path\\to\\a\\local\\git\\repo";

function App() {
  const [path, setPath] = useState(PLACEHOLDER_REPO_PATH);
  const [repoId, setRepoId] = useState<RepoId | null>(null);
  const [lastResult, setLastResult] = useState<unknown>(null);
  const [receivedEvents, setReceivedEvents] = useState<CheckCompletedPayload[]>(
    [],
  );

  // Subscribe to the typed repo:check-completed event for the lifetime of the
  // component; append each payload to the running list.
  useEffect(() => {
    const unlistenPromise = events.repoCheckCompleted.listen((event) => {
      setReceivedEvents((prev) => [...prev, event.payload]);
    });
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  async function handleAddRepo() {
    const result = await commands.repoAddPath(path);
    setLastResult(result);
    if (result.status === "ok") {
      setRepoId(result.data);
    }
  }

  async function handleCheckNow() {
    if (repoId === null) {
      setLastResult({ note: "Add a repo first to get a RepoId." });
      return;
    }
    const result = await commands.repoCheckNow(repoId);
    setLastResult(result);
  }

  return (
    <main className="min-h-svh bg-background p-6 text-foreground">
      <h1 className="text-xl font-semibold tracking-tight">
        RepoSync tracer (throwaway debug UI)
      </h1>
      <p className="mt-1 text-sm text-muted-foreground">
        E-12 tracer bullet. Delete when real screens land.
      </p>

      <section className="mt-6 flex flex-col gap-3">
        <label className="flex flex-col gap-1 text-sm">
          <span>Local repo path</span>
          <input
            className="rounded border border-input bg-transparent px-3 py-2 font-mono text-sm"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            spellCheck={false}
          />
        </label>

        <div className="flex gap-3">
          <button
            className="rounded bg-primary px-4 py-2 text-sm font-medium text-primary-foreground"
            onClick={() => void handleAddRepo()}
          >
            Add repo
          </button>
          <button
            className="rounded border border-input px-4 py-2 text-sm font-medium"
            onClick={() => void handleCheckNow()}
          >
            Check now
          </button>
        </div>

        <p className="text-sm">
          Current RepoId:{" "}
          <span className="font-mono">
            {repoId === null ? "(none)" : String(repoId)}
          </span>
        </p>
      </section>

      <section className="mt-6">
        <h2 className="text-sm font-semibold">Last command result</h2>
        <pre className="mt-2 overflow-auto rounded bg-muted p-3 text-xs">
          {JSON.stringify(lastResult, null, 2)}
        </pre>
      </section>

      <section className="mt-6">
        <h2 className="text-sm font-semibold">
          Received events ({receivedEvents.length})
        </h2>
        <pre className="mt-2 overflow-auto rounded bg-muted p-3 text-xs">
          {JSON.stringify(receivedEvents, null, 2)}
        </pre>
      </section>
    </main>
  );
}

export default App;
