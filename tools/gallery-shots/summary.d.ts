export type Theme = "light" | "dark";
export type ShotStatus = "same" | "changed" | "size-changed" | "added" | "removed";
export interface Size {
  width: number;
  height: number;
}
export interface ShotResult {
  section: string;
  theme: Theme;
  status: ShotStatus;
  changed: number;
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
): Omit<ShotResult, "section" | "theme">;
export function formatPercent(changed: number, total: number): string;
export function buildSummary(results: ShotResult[]): string;
