import { useEffect } from "react";
import { AppShell } from "@/components/app-shell";
import { ToastProvider } from "@/components/ui/toast";
import { cn } from "@/lib/utils";
import { DrawerBody, ScreenBody } from "@/gallery/demos";
import { LOOKS, ThemeContext } from "@/gallery/looks";
import { WindowContext, type FrameSpec } from "@/gallery/window-frame";

/**
 * One screen, full-bleed, as the document of an iframe. The Window control loads
 * this file into an iframe so the screen sees a real CSS viewport of the chosen
 * size. `.dark` and `data-look` go on the same element, as everywhere else.
 */
export function FrameView({ spec }: { spec: FrameSpec }) {
  useEffect(() => {
    document.documentElement.classList.toggle("dark", spec.dark);
  }, [spec.dark]);
  const look = LOOKS.find((l) => l.id === spec.look)?.attribute;
  return (
    <ThemeContext.Provider value={spec.dark}>
      <WindowContext.Provider value={{ windowId: "natural", scenario: spec.scenario, look: spec.look }}>
        <div
          data-look={look}
          data-testid="frame-root"
          className={cn("h-screen overflow-auto bg-background text-foreground", spec.dark && "dark")}
        >
          <ToastProvider>
            {spec.screen === "shell" && <AppShell />}
            {spec.screen === "drawer" && <DrawerBody />}
            {(spec.screen === "dashboard" ||
              spec.screen === "repos" ||
              spec.screen === "activity" ||
              spec.screen === "settings") && <ScreenBody screen={spec.screen} controls={false} />}
          </ToastProvider>
        </div>
      </WindowContext.Provider>
    </ThemeContext.Provider>
  );
}
