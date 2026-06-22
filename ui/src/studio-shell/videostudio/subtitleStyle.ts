/**
 * Subtitle look shared between the preview and the libass export, so what you
 * see is what renders. These MUST mirror the Rust constants in
 * `crates/studio/src/dub/subtitle.rs` (PLAY_H, SIZE_FACTOR, MAX_WIDTH_FRAC,
 * MARGIN_V_FRAC, FONT_NAME). Sizes are authored against a 1080-high reference
 * and scaled to the actual stage via container-query height units (`cqh`),
 * exactly like libass scales PlayResY to the frame.
 */
import type { CSSProperties } from "react";

export const SUB_FONT = '"Be Vietnam Pro SemiBold", system-ui, sans-serif';
const PLAY_H = 1080;
const SIZE_FACTOR = 1.6; // sub_size slider → px @1080
const MAX_WIDTH_FRAC = 0.86; // wrap width; rest is side margin
const MARGIN_V_FRAC = 0.075; // distance from the bottom

/** Font size as a fraction of stage height (needs `container-type: size` on an
 *  ancestor). `1cqh` = 1% of the container's height. */
export function subFontSize(subSize: number): string {
  const pctOfHeight = (subSize * SIZE_FACTOR) / PLAY_H; // 0..1
  return `calc(${(pctOfHeight * 100).toFixed(3)} * 1cqh)`;
}

/** A crisp dark outline approximating libass `Outline` (BorderStyle 1). */
export const SUB_OUTLINE =
  "0 0 calc(.18*1cqh) #000, 0 0 calc(.18*1cqh) #000, " +
  "calc(.12*1cqh) calc(.12*1cqh) calc(.18*1cqh) #000, " +
  "calc(-.12*1cqh) calc(-.12*1cqh) calc(.18*1cqh) #000";

/** Wrapper box for the subtitle block: bottom-centred, wrapped within margins. */
export function subBoxStyle(): CSSProperties {
  const sidePct = ((1 - MAX_WIDTH_FRAC) / 2) * 100;
  return {
    position: "absolute",
    left: `${sidePct}%`,
    right: `${sidePct}%`,
    bottom: `${MARGIN_V_FRAC * 100}%`,
    textAlign: "center",
    pointerEvents: "none",
    lineHeight: 1.2,
  };
}
