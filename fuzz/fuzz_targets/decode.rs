//! PT: Fuzzing do codec: bytes arbitrários viram tanto o texto colado quanto o JSON dentro da
//!     string. Nenhuma entrada pode causar pânico ou passar do limite de memória do libFuzzer.
//! EN: Codec fuzzing: arbitrary bytes become both the pasted text and the JSON inside the string.
//!     No input may panic or exceed libFuzzer's memory limit.
#![no_main]

use bf_blueprint::{Limits, decode, encode_raw_json};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = decode(text, &limits);
        if let Ok(wrapped) = encode_raw_json(text) {
            let _ = decode(&wrapped, &limits);
        }
    }
});
