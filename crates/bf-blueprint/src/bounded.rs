//! PT: Desserialização com teto fixo: listas e mapas param de crescer e devolvem erro assim que
//!     passam do limite, sem alocar o resto da entrada. O limite atingido é registrado num valor
//!     por thread (não na mensagem de erro), para que texto do usuário não consiga forjá-lo.
//! EN: Capped deserialization: lists and maps stop growing and return an error as soon as they
//!     exceed the limit, without allocating the rest of the input. The hit limit is recorded in a
//!     per-thread value (not in the error message), so user text cannot forge it.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserialize, Deserializer, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// PT: Capacidade inicial máxima reservada, para que um `size_hint` falso não aloque demais.
/// EN: Maximum initial reserved capacity, so that a fake `size_hint` cannot over-allocate.
const MAX_INITIAL_CAPACITY: usize = 1_024;

thread_local! {
    static LIMIT_HIT: Cell<Option<usize>> = const { Cell::new(None) };
}

/// PT: Registra que o limite `max` foi atingido e devolve o erro genérico do serde.
/// EN: Records that limit `max` was hit and returns the generic serde error.
pub fn limit_error<E: Error>(max: usize) -> E {
    LIMIT_HIT.with(|hit| hit.set(Some(max)));
    E::custom("limit exceeded")
}

/// PT: Lê e limpa o limite registrado nesta thread.
/// EN: Reads and clears the limit recorded on this thread.
pub fn take_limit_hit() -> Option<usize> {
    LIMIT_HIT.with(Cell::take)
}

/// PT: Lê uma lista com no máximo `MAX` elementos.
/// EN: Reads a list with at most `MAX` elements.
pub fn bounded_vec<'de, D, T, const MAX: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedVisitor<T, const MAX: usize>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for BoundedVisitor<T, MAX> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(
                formatter,
                "uma lista com até {MAX} elementos / a list with up to {MAX} elements"
            )
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let hint = seq.size_hint().unwrap_or(0);
            let mut values = Vec::with_capacity(hint.min(MAX).min(MAX_INITIAL_CAPACITY));
            while let Some(value) = seq.next_element()? {
                if values.len() >= MAX {
                    return Err(limit_error(MAX));
                }
                values.push(value);
            }
            Ok(values)
        }

        // PT: O Lua serializa tabela vazia como `{}`; aceita como lista vazia.
        // EN: Lua serializes an empty table as `{}`; accept it as an empty list.
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            if map.next_key::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("esperava lista / expected a list"));
            }
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(BoundedVisitor::<T, MAX>(PhantomData))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct Holder {
        #[serde(deserialize_with = "super::bounded_vec::<_, _, 3>")]
        values: Vec<u32>,
    }

    #[test]
    fn accepts_up_to_the_limit() {
        let holder: Holder = serde_json::from_str(r#"{"values":[1,2,3]}"#).unwrap();
        assert_eq!(holder.values, vec![1, 2, 3]);
        let empty: Holder = serde_json::from_str(r#"{"values":{}}"#).unwrap();
        assert!(empty.values.is_empty());
        assert_eq!(super::take_limit_hit(), None);
    }

    #[test]
    fn rejects_above_the_limit() {
        assert!(serde_json::from_str::<Holder>(r#"{"values":[1,2,3,4]}"#).is_err());
        assert_eq!(super::take_limit_hit(), Some(3));
        assert_eq!(super::take_limit_hit(), None);
    }
}
