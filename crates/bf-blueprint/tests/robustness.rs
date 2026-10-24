//! PT: Robustez com entradas aleatórias (fuzzing no estável, via proptest): nenhuma entrada pode
//!     causar pânico; ou dá certo, ou devolve um erro com código.
//! EN: Robustness with random inputs (stable-toolchain fuzzing, via proptest): no input may cause
//!     a panic; it either succeeds or returns an error with a code.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use bf_blueprint::{Limits, decode, encode_raw_json};
use proptest::prelude::*;

// PT: JSON de blueprint válido usado como base para as mutações.
// EN: Valid blueprint JSON used as the base for mutations.
const BASE: &str = r#"{"blueprint":{"item":"blueprint","version":562954249895936,"label":"x",
"entities":[{"entity_number":1,"name":"inserter","position":{"x":0.5,"y":0.5},"direction":4,
"items":[{"id":{"name":"speed-module"},"items":{"in_inventory":[{"inventory":4,"stack":0}]}}]},
{"entity_number":2,"name":"small-electric-pole","position":{"x":2.5,"y":0.5}}],
"wires":[[1,5,2,5]],"tiles":[{"name":"concrete","position":{"x":0,"y":0}}]}}"#;

fn check(text: &str) {
    match decode(text, &Limits::default()) {
        Ok(_) => {}
        Err(error) => assert!(error.code().starts_with("blueprint.")),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_text_never_panics(text in ".{0,400}") {
        check(&text);
        check(&format!("0{text}"));
    }

    #[test]
    fn arbitrary_base64_never_panics(body in "[A-Za-z0-9+/=]{0,600}") {
        check(&format!("0{body}"));
    }

    #[test]
    fn arbitrary_json_never_panics(json in "[\\[\\]{}\":,0-9a-z_ .-]{0,400}") {
        check(&encode_raw_json(&json).unwrap());
    }

    #[test]
    fn mutated_blueprints_never_panic(position in 0usize..BASE.len(), replacement in "[\\[\\]{}\":,0-9a-z_ .-]{0,6}") {
        let mut json = BASE.to_owned();
        if json.is_char_boundary(position) {
            let end = (position + 3).min(json.len());
            if json.is_char_boundary(end) {
                json.replace_range(position..end, &replacement);
            }
        }
        check(&encode_raw_json(&json).unwrap());
    }

    #[test]
    fn arbitrary_numbers_never_panic(x in any::<f64>(), number in any::<u64>(), direction in any::<u64>()) {
        let json = format!(
            r#"{{"blueprint":{{"item":"blueprint","version":562954249895936,"entities":[{{"entity_number":{number},"name":"inserter","position":{{"x":{x},"y":0}},"direction":{direction}}}]}}}}"#
        );
        check(&encode_raw_json(&json).unwrap());
    }
}

#[test]
fn base_blueprint_is_valid() {
    let decoded = decode(&encode_raw_json(BASE).unwrap(), &Limits::default()).unwrap();
    let bf_blueprint::BlueprintString::Blueprint(blueprint) = decoded.content else {
        unreachable!()
    };
    assert_eq!(blueprint.entities.len(), 2);
    assert_eq!(
        blueprint.entities[0].items.as_ref().unwrap().counts()["speed-module"],
        1
    );
}
