//! PT: Pico de memória com entradas feitas para amplificar alocação (achados do code review da
//!     Fase 1). O alocador do `dhat` mede o maior uso de heap do processo inteiro.
//! EN: Peak memory with inputs crafted to amplify allocation (Phase 1 code review findings).
//!     The `dhat` allocator measures the largest heap use of the whole process.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use bf_blueprint::{Limits, decode, encode_raw_json};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// PT: Teto aceito para o pico: o texto descomprimido (até 32 MB) mais as cópias cruas.
/// EN: Accepted peak cap: the decompressed text (up to 32 MB) plus the raw copies.
const MAX_PEAK_BYTES: usize = 160 * 1024 * 1024;

// PT: Texto pseudoaleatório que não comprime bem, para driblar a checagem de razão.
// EN: Pseudo-random text that does not compress well, to get past the ratio check.
fn noise(len: usize) -> String {
    let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(alphabet[(state % alphabet.len() as u64) as usize])
        })
        .collect()
}

#[test]
fn amplification_attacks_stay_below_the_cap() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let pad = noise(250_000);
    let junk = "0,".repeat(13_000_000);

    // PT: Campo desconhecido gigante na entidade. EN: Huge unknown field on the entity.
    let entity_attack = format!(
        r#"{{"blueprint":{{"item":"blueprint","version":562954249895936,"entities":[{{"entity_number":1,"name":"wooden-chest","position":{{"x":0.5,"y":0.5}},"pad":"{pad}","junk":[{junk}0]}}]}}}}"#
    );
    let text = encode_raw_json(&entity_attack).unwrap();
    drop(entity_attack);
    let _ = decode(&text, &Limits::default());
    drop(text);

    // PT: O mesmo array em `schedules`. EN: The same array in `schedules`.
    let schedules_attack = format!(
        r#"{{"blueprint":{{"item":"blueprint","version":562954249895936,"label":"{pad}","schedules":[{junk}0]}}}}"#
    );
    let text = encode_raw_json(&schedules_attack).unwrap();
    drop(schedules_attack);
    let _ = decode(&text, &Limits::default());
    drop(text);
    drop(junk);

    let stats = dhat::HeapStats::get();
    assert!(
        stats.max_bytes < MAX_PEAK_BYTES,
        "pico de memória / memory peak: {} MB",
        stats.max_bytes / (1024 * 1024)
    );
    eprintln!("pico / peak: {} MB", stats.max_bytes / (1024 * 1024));
}
