export type Theme = "light" | "dark";
export const NOISE_FLOOR: number;
export type ShotStatus = "same" | "noise" | "changed" | "size-changed" | "added" | "removed";
export interface Size {
  width: number;
  height: number;
}
export interface ShotResult {
  section: string;
  theme: Theme;
  status: ShotStatus;
  /** Raw count (pixelmatch threshold 0). Decides the status. */
  changed: number;
  /** Perceptual count (threshold 0.1). Reported beside `changed` when they differ. */
  perceptual?: number;
  total: number;
  baseSize?: string;
  headSize?: string;
}
export function shotName(sectionId: string, theme: Theme): string;
export function parseShotName(fileName: string): { section: string; theme: Theme } | null;
export function classifyShot(
  base: Size | null,
  head: Size | null,
  changed?: number,
  noiseFloor?: number,
  perceptual?: number,
): Omit<ShotResult, "section" | "theme">;
export function formatPercent(changed: number, total: number): string;
export function buildSummary(results: ShotResult[]): string;
