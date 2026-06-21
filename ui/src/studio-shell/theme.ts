// Design tokens for the "Beesoft Studio" shell + Video Studio editor.
// Ported verbatim from the VieNeu Dubbing / Video Studio design handoff so the
// implementation matches the mockup palette and typography exactly. These are
// intentionally raw hex/inline values (not Tailwind tokens) because the shell is
// a faithful visual port that lives alongside — not inside — the Tailwind UI.

export const FONT = "'Barlow','PingFang SC','Noto Sans SC',sans-serif";
export const MONO = "'IBM Plex Mono',monospace";

/** Shell + editor colour palette. */
export const C = {
  // surfaces
  appBg: "#15161d",
  titlebar: "#17151f",
  content: "#191722",
  panel: "#1F1D2B",
  panel2: "#252836",
  panel3: "#2D303E",
  inset: "#1c1a27",
  insetDark: "#15131d",
  card: "#1F1D2B",
  cardHover: "#22202e",
  laneBg: "#191722",
  ruler: "#211f2d",
  previewBg: "#22242f",
  checker: "#1c1e27",
  checkerAlt: "#202330",
  // lines
  border: "#393C49",
  borderSoft: "#2a2c38",
  borderTab: "#26242f",
  borderInset: "#2f3140",
  borderInset2: "#2a2833",
  tick: "#34384a",
  // text
  ink: "#fff",
  muted: "#8b8f9e",
  muted2: "#889898",
  muted3: "#6b6f82",
  muted4: "#7c8094",
  muted5: "#5a5e70",
  faint: "#52566a",
  steel: "#ABBBC2",
  ink2: "#9fa3b5",
  ink3: "#c7cbd8",
  ink4: "#cfd3df",
  ink5: "#e9edf0",
  // accents
  purple: "#9288E0",
  purpleLt: "#b3aaf0",
  purpleDk: "#6f64c4",
  coral: "#EA7C69",
  coralLt: "#ef8876",
  teal: "#50D1AA",
  blue: "#65B0F6",
  orange: "#FFB572",
  pink: "#FF7CA3",
} as const;

const STYLE_ID = "beesoft-studio-styles";

/** Inject the Google Fonts link + base CSS (scrollbars, range inputs, spin) once. */
export function injectStudioStyles(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(STYLE_ID)) return;

  if (!document.querySelector('link[data-beesoft-fonts]')) {
    const pre1 = document.createElement("link");
    pre1.rel = "preconnect";
    pre1.href = "https://fonts.googleapis.com";
    const pre2 = document.createElement("link");
    pre2.rel = "preconnect";
    pre2.href = "https://fonts.gstatic.com";
    pre2.crossOrigin = "anonymous";
    const font = document.createElement("link");
    font.rel = "stylesheet";
    font.dataset.beesoftFonts = "1";
    font.href =
      "https://fonts.googleapis.com/css2?family=Barlow:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500;600&display=swap";
    document.head.append(pre1, pre2, font);
  }

  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
.bss, .bss *{box-sizing:border-box;}
.bss ::-webkit-scrollbar{width:10px;height:10px;}
.bss ::-webkit-scrollbar-thumb{background:#393C49;border-radius:5px;border:2px solid transparent;background-clip:padding-box;}
.bss ::-webkit-scrollbar-thumb:hover{background:#4a4e5e;background-clip:padding-box;}
.bss ::-webkit-scrollbar-track{background:transparent;}
.bss .tabstrip::-webkit-scrollbar{height:0;}
.bss .noscroll::-webkit-scrollbar{height:0;}
.bss input[type=range]{-webkit-appearance:none;appearance:none;height:4px;border-radius:3px;outline:none;cursor:pointer;}
.bss input[type=range]::-webkit-slider-thumb{-webkit-appearance:none;width:13px;height:13px;border-radius:50%;background:#fff;border:2px solid #EA7C69;box-shadow:0 1px 4px rgba(0,0,0,.5);transition:transform .12s;}
.bss input[type=range]:hover::-webkit-slider-thumb{transform:scale(1.18);}
.bss input[type=range]::-moz-range-thumb{width:13px;height:13px;border:2px solid #EA7C69;border-radius:50%;background:#fff;}
.bss input.num::-webkit-outer-spin-button,.bss input.num::-webkit-inner-spin-button{-webkit-appearance:none;margin:0;}
.bss input.num{-moz-appearance:textfield;}
.bss input::placeholder{color:#5a5e70;}
@keyframes bss-spin{to{transform:rotate(360deg);}}
`;
  document.head.appendChild(style);
}
