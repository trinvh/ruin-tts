//! Text handling: emotion-aware phonemization, chunking, and audio joining.
//!
//! Ports `phonemize_text_with_emotions` (phonemize_text.py) and
//! `split_text_into_chunks` / `join_audio_chunks` (core_utils.py).

use once_cell::sync::Lazy;
use regex::Regex;
use sea_g2p_rs::Pipeline;

// ── Inline non-verbal cues ────────────────────────────────────────────────
static EMOTION_SPLIT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\[[^\]]+\]|<\|emotion_\d+\|>)").unwrap());

const ATTACHING_PUNCT: &[char] = &[
    '.', ',', '!', '?', ';', ':', '…', ')', ']', '}', '"', '\'', '’', '”',
];
const TERMINAL_PUNCT: &[char] = &['.', '!', '?'];
const WEAK_TRAILING: &[char] = &[',', ';', ':', '…', ' ', '\t'];

fn emotion_tag_token(tag: &str) -> Option<String> {
    let t = tag.trim();
    if t.starts_with("<|") {
        return Some(t.to_string());
    }
    // strip surrounding [ ]
    let inner = t
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
        .to_lowercase();
    let k = match inner.as_str() {
        "chuckle" | "cười" | "cuoi" => 1,
        "sigh" | "thở dài" | "tho dai" => 2,
        "clear throat" | "hắng giọng" | "hang giong" => 3,
        _ => return None,
    };
    Some(format!("<|emotion_{k}|>"))
}

fn ensure_terminal_punct(phones: &str) -> String {
    let s = phones.trim_end();
    if s.is_empty() {
        return s.to_string();
    }
    if let Some(last) = s.chars().last() {
        if TERMINAL_PUNCT.contains(&last) {
            return s.to_string();
        }
    }
    let s2 = s.trim_end_matches(WEAK_TRAILING);
    if s2.is_empty() {
        phones.to_string()
    } else {
        format!("{s2}.")
    }
}

/// Phonemize `text`, keeping inline cues `[cười]`/`[thở dài]`/`[hắng giọng]`
/// (or English / explicit `<|emotion_k|>`) as emotion tokens. Mirrors the
/// reference spacing exactly.
pub fn phonemize_with_emotions(pipe: &Pipeline, text: &str) -> String {
    if !text.contains('[') && !text.contains("<|emotion_") {
        return pipe.run(text, true); // fast path: punc_norm = true
    }

    let mut out = String::new();
    let mut last_end = 0usize;
    let push_fragment = |out: &mut String, frag: &str| {
        if frag.trim().is_empty() {
            return;
        }
        let ph = pipe.run(frag, false); // fragments: no punc_norm
        if ph.is_empty() {
            return;
        }
        if out.is_empty() {
            out.push_str(&ph);
        } else if ph.starts_with(ATTACHING_PUNCT) {
            out.push_str(&ph);
        } else {
            out.push(' ');
            out.push_str(&ph);
        }
    };

    for m in EMOTION_SPLIT.find_iter(text) {
        // fragment before the tag
        push_fragment(&mut out, &text[last_end..m.start()]);
        // the tag itself
        if let Some(tok) = emotion_tag_token(m.as_str()) {
            if out.is_empty() {
                out = tok;
            } else {
                out.push(' ');
                out.push_str(&tok);
            }
        } else {
            // unrecognized bracketed span → phonemize as ordinary text
            push_fragment(&mut out, m.as_str());
        }
        last_end = m.end();
    }
    push_fragment(&mut out, &text[last_end..]);

    ensure_terminal_punct(&out)
}

// ── Raw text chunking (split_text_into_chunks) ────────────────────────────

/// Split on a whitespace run whose preceding non-space char is in `punct`,
/// dropping the whitespace (mirrors the lookbehind regex `(?<=[punct])\s+`).
fn split_after_punct(text: &str, punct: &[char]) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            let prev = cur.chars().rev().find(|p| !p.is_whitespace());
            if let Some(p) = prev {
                if punct.contains(&p) {
                    while i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                    out.push(std::mem::take(&mut cur));
                    continue;
                }
            }
            cur.push(c);
            i += 1;
        } else {
            cur.push(c);
            i += 1;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

const SENTENCE_END: &[char] = &['.', '!', '?', '…'];
const MINOR_PUNCT: &[char] = &[',', ';', ':', '-', '–', '—'];

/// Port of `split_text_into_chunks`: split raw text into chunks ≤ `max_chars`.
pub fn split_text_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut final_chunks: Vec<String> = Vec::new();

    for para in text.trim().split(|c| c == '\n' || c == '\r') {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let sentences = split_after_punct(para, SENTENCE_END);
        let mut buffer = String::new();

        for sentence in sentences {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }
            if sentence.chars().count() > max_chars {
                if !buffer.is_empty() {
                    final_chunks.push(std::mem::take(&mut buffer));
                }
                for part in split_after_punct(sentence, MINOR_PUNCT) {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    if buffer.chars().count() + 1 + part.chars().count() <= max_chars {
                        if buffer.is_empty() {
                            buffer = part.to_string();
                        } else {
                            buffer.push(' ');
                            buffer.push_str(part);
                        }
                    } else {
                        if !buffer.is_empty() {
                            final_chunks.push(std::mem::take(&mut buffer));
                        }
                        buffer = part.to_string();
                        if buffer.chars().count() > max_chars {
                            // word-level fallback
                            let mut current = String::new();
                            for word in buffer.split_whitespace() {
                                if !current.is_empty()
                                    && current.chars().count() + 1 + word.chars().count()
                                        > max_chars
                                {
                                    final_chunks.push(std::mem::take(&mut current));
                                    current = word.to_string();
                                } else if current.is_empty() {
                                    current = word.to_string();
                                } else {
                                    current.push(' ');
                                    current.push_str(word);
                                }
                            }
                            buffer = current;
                        }
                    }
                }
            } else if !buffer.is_empty()
                && buffer.chars().count() + 1 + sentence.chars().count() > max_chars
            {
                final_chunks.push(std::mem::replace(&mut buffer, sentence.to_string()));
            } else if buffer.is_empty() {
                buffer = sentence.to_string();
            } else {
                buffer.push(' ');
                buffer.push_str(sentence);
            }
        }
        if !buffer.is_empty() {
            final_chunks.push(buffer);
        }
    }

    final_chunks
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

// ── Audio join (join_audio_chunks) ────────────────────────────────────────

/// Concatenate chunk waveforms with optional silence or crossfade between them.
pub fn join_audio_chunks(
    chunks: &[Vec<f32>],
    sample_rate: u32,
    silence_p: f32,
    crossfade_p: f32,
) -> Vec<f32> {
    if chunks.is_empty() {
        return Vec::new();
    }
    if chunks.len() == 1 {
        return chunks[0].clone();
    }
    let silence_samples = (sample_rate as f32 * silence_p) as usize;
    let crossfade_samples = (sample_rate as f32 * crossfade_p) as usize;
    let mut out = chunks[0].clone();

    for next in &chunks[1..] {
        if silence_samples > 0 {
            out.extend(std::iter::repeat(0.0f32).take(silence_samples));
            out.extend_from_slice(next);
        } else if crossfade_samples > 0 {
            let overlap = crossfade_samples.min(out.len()).min(next.len());
            if overlap > 0 {
                let base = out.len() - overlap;
                for j in 0..overlap {
                    let fade_out = 1.0 - j as f32 / overlap as f32;
                    let fade_in = j as f32 / overlap as f32;
                    out[base + j] = out[base + j] * fade_out + next[j] * fade_in;
                }
                out.extend_from_slice(&next[overlap..]);
            } else {
                out.extend_from_slice(next);
            }
        } else {
            out.extend_from_slice(next);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_max_chars() {
        let text = "Câu một. Câu hai! Câu ba? Câu bốn.";
        let chunks = split_text_into_chunks(text, 12);
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert!(
                c.chars().count() <= 12 || c.split_whitespace().count() == 1,
                "chunk too long: {c:?}"
            );
        }
    }

    #[test]
    fn join_inserts_silence() {
        let a = vec![1.0f32; 10];
        let b = vec![2.0f32; 10];
        let out = join_audio_chunks(&[a, b], 100, 0.1, 0.0);
        assert_eq!(out.len(), 10 + 10 + 10); // 0.1s * 100 = 10 silence
    }
}
