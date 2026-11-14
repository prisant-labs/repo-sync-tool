// Order matters: the IPC stub must be installed before any component module
// that could call `invoke` is evaluated. ES imports run in source order.
import "@/gallery/stub-ipc";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { GalleryApp } from "@/gallery/gallery-app";
import "@/gallery/gallery.css";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Root element #root not found");
}

createRoot(rootElement).render(
  <StrictMode>
    <GalleryApp />
  </StrictMode>,
);
