import { createContext } from "react";

/**
 * The window sizes the Screens cells can be framed at.
 *
 * `natural` is today's behaviour: the cell is as wide as the gallery page.
 * `w1024` is a 1024 CSS pixel window at 100% display scaling.
 *
 * `w1024s150` reproduces what Windows does to a 1024 PHYSICAL pixel window at
 * 150% display scaling: the page lays out in 1024 / 1.5 = 682.67 CSS pixels (683 as Windows rounds it) and every
 * pixel is drawn 1.5x larger. CSS `zoom: 1.5` on a 682.67px frame gives the same two
 * numbers (about 683 pixels of layout, exactly 1024 on screen), and it scales text
 * the same way. A `transform: scale` would also look right but leaves the layout box
 * unscaled, which is the wrong box to scroll and to clip.
 *
 * Limit: the frame changes layout width only. A viewport media query such as
 * `md:` still reads the gallery page, not the frame, so a breakpoint can disagree
 * with the real window. The Dashboard's `md:grid-cols-4` is the one case today.
 */
export const WINDOWS = [
  { id: "natural", label: "Natural", cssWidth: null, zoom: 1 },
  { id: "w1024", label: "1024px", cssWidth: 1024, zoom: 1 },
  { id: "w1024s150", label: "1024px at 150%", cssWidth: 1024 / 1.5, zoom: 1.5 },
] as const;

export type WindowId = (typeof WINDOWS)[number]["id"];

export const WindowContext = createContext<WindowId>("natural");
