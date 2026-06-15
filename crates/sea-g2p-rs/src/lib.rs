//! sea-g2p-rs — native Rust core of [sea-g2p](https://github.com/pnnbao97/sea-g2p).
//!
//! This is a PyO3-free fork of the upstream Apache-2.0 crate, exposing the
//! grapheme-to-phoneme engine and the Vietnamese text normalizer directly to
//! Rust callers (no Python interop). The 48 MB binary phoneme dictionary is
//! embedded into the binary, so [`Pipeline::new`] needs no external files.
//!
//! ```no_run
//! use sea_g2p_rs::Pipeline;
//! let p = Pipeline::new().unwrap();
//! let phones = p.run("Xin chào Việt Nam", true);
//! ```

pub mod g2p;
pub mod punc;
pub mod vi_normalizer;

pub use g2p::G2PEngine;
pub use vi_normalizer::Normalizer;

/// The phoneme dictionary, embedded so the engine is self-contained.
pub static EMBEDDED_DICT: &[u8] = include_bytes!("../assets/sea_g2p.bin");

/// Full text→phoneme pipeline: normalize (numbers, dates, units, …) then G2P.
///
/// Mirrors the upstream Python `SEAPipeline`: `run` == normalize + phonemize.
pub struct Pipeline {
    normalizer: Normalizer,
    g2p: G2PEngine,
}

impl Pipeline {
    /// Build a pipeline using the embedded Vietnamese dictionary.
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            normalizer: Normalizer::new("vi"),
            g2p: G2PEngine::from_bytes(Box::from(EMBEDDED_DICT))?,
        })
    }

    /// Build a pipeline from an external `sea_g2p.bin` on disk.
    pub fn from_dict_path(path: &str) -> std::io::Result<Self> {
        Ok(Self {
            normalizer: Normalizer::new("vi"),
            g2p: G2PEngine::new(path)?,
        })
    }

    /// Normalize then phonemize a single string.
    ///
    /// When `punc_norm` is true the trailing punctuation is normalized during
    /// the normalization step (a sentence ends with a single `.`; a short
    /// sentence — fewer than 5 words — is forced to `.`).
    pub fn run(&self, text: &str, punc_norm: bool) -> String {
        if text.is_empty() {
            return String::new();
        }
        let normalized = self.normalizer.normalize(text, punc_norm);
        self.g2p.phonemize(&normalized)
    }

    /// Phonemize an already-normalized string (G2P only).
    pub fn phonemize_only(&self, normalized_text: &str) -> String {
        self.g2p.phonemize(normalized_text)
    }

    /// Access the underlying normalizer (e.g. to normalize without phonemizing).
    pub fn normalizer(&self) -> &Normalizer {
        &self.normalizer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonemizes_like_python_reference() {
        let p = Pipeline::new().expect("load embedded dict");
        let cases = [
            ("Xin chào Việt Nam", "sˈin tʃˈaː2w vˈiɛ6t̪ nˈaːm."),
            ("Nghe hay quá đi.", "ŋˈɛ hˈaj kwˈaːɜ ɗˈi."),
            (
                "Giá SP500 hôm nay là 4.200,5 điểm.",
                "zˈaːɜ ˈɛɜt̪ pˈe nˈam tʃˈam hˈom nˈaj lˌaː2 bˈoɜn ŋˈi2n hˈaːj tʃˈam fˈəɪ4 nˈam ɗˈiɛ4m.",
            ),
        ];
        for (input, expected) in cases {
            let got = p.run(input, true);
            assert_eq!(got, expected, "mismatch for input {input:?}");
        }
    }
}
