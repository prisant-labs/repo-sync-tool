// Order matters: the IPC stub must be installed before any component module
// that could call `invoke` is evaluated. ES imports run in source order.
import { setScenario } from "@/gallery/stub-ipc";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { GalleryApp } from "@/gallery/gallery-app";
import { FrameView } from "@/gallery/frame-view";
import { parseFrameHash } from "@/gallery/window-frame";
import "@/gallery/gallery.css";

// A location hash that names a screen makes this document ONE screen, full-bleed:
// the Window control loads it into an iframe to get a viewport of a chosen size.
const frame = parseFrameHash(window.location.hash);
if (frame) setScenario(frame.scenario);

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Root element #root not found");
}

createRoot(rootElement).render(
  <StrictMode>
    {frame ? <FrameView spec={frame} /> : <GalleryApp />}
  </StrictMode>,
);
