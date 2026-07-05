import { AppShell } from "@/components/app-shell";
import { ToastProvider } from "@/components/ui/toast";

// Real application shell. Replaces the E-12 throwaway tracer: the shell and its
// screens talk to the backend only through the generated tauri-specta bindings
// (via the typed hooks in @/hooks/queries), never raw invoke/listen. ToastProvider
// wraps everything so action feedback (useToast) is available in every screen.
export default function App() {
  return (
    <ToastProvider>
      <AppShell />
    </ToastProvider>
  );
}
