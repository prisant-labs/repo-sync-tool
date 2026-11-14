import { createContext } from "react";

/**
 * The looks the gallery can show. `today` sets no `data-look` attribute at all,
 * which is exactly what the shipped app does, so it is the control: whatever the
 * other two change is measured against real, unmodified rendering.
 */
export const LOOKS = [
  { id: "today", label: "Today", attribute: undefined },
  { id: "graphite", label: "Look A (graphite)", attribute: "graphite" },
  { id: "slate", label: "Look B (slate)", attribute: "slate" },
] as const;

export type LookId = (typeof LOOKS)[number]["id"];

/** The control bar's light/dark switch, for sections that draw their own theme roots. */
export const ThemeContext = createContext(false);
