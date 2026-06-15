import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { defaultOutputDir } from "./platform";

// TTS output settings shared across routes (the single queue lives on the
// Studio page; Settings can edit these too). Persisted to localStorage.
type TtsSettings = {
  outputDir: string;
  setOutputDir: (d: string) => void;
  concurrency: number;
  setConcurrency: (n: number) => void;
};

const Ctx = createContext<TtsSettings | null>(null);

export function TtsSettingsProvider({ children }: { children: ReactNode }) {
  const [outputDir, setOutputDirState] = useState("");
  const [concurrency, setConcurrencyState] = useState(() => {
    const v = Number(localStorage.getItem("vieneu_concurrency"));
    return v >= 1 && v <= 8 ? v : 2;
  });

  // Resolve the default output folder (Downloads) once if none saved.
  useEffect(() => {
    const saved = localStorage.getItem("vieneu_out");
    if (saved) {
      setOutputDirState(saved);
      return;
    }
    defaultOutputDir().then((d) => setOutputDirState(d ?? ""));
  }, []);

  const setOutputDir = (d: string) => {
    setOutputDirState(d);
    localStorage.setItem("vieneu_out", d);
  };
  const setConcurrency = (n: number) => {
    const c = Math.max(1, Math.min(8, n || 1));
    setConcurrencyState(c);
    localStorage.setItem("vieneu_concurrency", String(c));
  };

  return (
    <Ctx.Provider value={{ outputDir, setOutputDir, concurrency, setConcurrency }}>
      {children}
    </Ctx.Provider>
  );
}

export function useTtsSettings(): TtsSettings {
  const v = useContext(Ctx);
  if (!v) throw new Error("useTtsSettings must be used within TtsSettingsProvider");
  return v;
}
