import { createContext } from "react";
import type { Scenario } from "@/gallery/fixtures";
import type { LookId } from "@/gallery/looks";

/**
 * The window sizes the Screens cells can be framed at.
 *
 * `natural` is today's behaviour: the cell is as wide as the gallery page.
 *
 * The framed modes draw the screen in an IFRAME, because only an iframe has its
 * own CSS viewport. A plain `width` on a div does not: the drawer's `vw` widths,
 * the dialogs' `vh` heights, the shell's `svh` and the Dashboard's `md:` breakpoint
 * all read the viewport, so they would keep reading the outer gallery window.
 *
 * `w1024` is a 1024 CSS pixel window at 100% display scaling: a 1024 wide iframe.
 *
 * `w1024s150` reproduces what Windows does to a 1024 PHYSICAL pixel window at
 * 150% display scaling: the page lays out in 683 CSS pixels (1024 / 1.5, rounded
 * as Windows rounds it) and every pixel is drawn 1.5x larger. The iframe is 683
 * wide and is presented at 1.5x by an external `transform: scale(1.5)` with its
 * origin at the top left; a wrapper is sized to the scaled box so the page flows
 * around the right number of pixels.
 *
 * The iframe loads this same built file with a location hash that names one
 * screen, so it works under `file://`, where there is no server and no query
 * string a build could rely on.
 */
export const WINDOWS = [
  { id: "natural", label: "Natural", cssWidth: null, scale: 1 },
  { id: "w1024", label: "1024px", cssWidth: 1024, scale: 1 },
  { id: "w1024s150", label: "1024px at 150%", cssWidth: 683, scale: 1.5 },
] as const;

export type WindowId = (typeof WINDOWS)[number]["id"];

/** What a framed cell needs to know about the gallery's controls. */
export interface FrameControls {
  windowId: WindowId;
  scenario: Scenario;
  look: LookId;
}

export const WindowContext = createContext<FrameControls>({ windowId: "natural", scenario: "populated", look: "today" });

export const FRAME_SCREENS = ["dashboard", "repos", "activity", "settings", "drawer", "shell"] as const;
export type FrameScreen = (typeof FRAME_SCREENS)[number];

export interface FrameSpec {
  screen: FrameScreen;
  scenario: Scenario;
  look: LookId;
  dark: boolean;
}

export function encodeFrameHash(spec: FrameSpec): string {
  const p = new URLSearchParams({ frame: spec.screen, state: spec.scenario, look: spec.look, dark: spec.dark ? "1" : "0" });
  return `#${p.toString()}`;
}

/** Null when the hash does not name a framed screen: the normal gallery loads. */
export function parseFrameHash(hash: string): FrameSpec | null {
  const p = new URLSearchParams(hash.replace(/^#/, ""));
  const screen = p.get("frame");
  if (!(FRAME_SCREENS as readonly string[]).includes(screen ?? "")) return null;
  const scenario = p.get("state") as Scenario | null;
  const look = p.get("look") as LookId | null;
  return {
    screen: screen as FrameScreen,
    scenario: scenario ?? "populated",
    look: look ?? "today",
    dark: p.get("dark") === "1",
  };
}
