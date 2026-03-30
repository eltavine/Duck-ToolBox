use anyhow::{Context, Result, bail};
use ciborium::{
    de::from_reader,
    ser::into_writer,
    value::{Integer, Value},
};

pub fn encode(value: &Value) -> Result<Vec<u8>> {
    let value = canonicalize(value)?;
    let mut bytes = Vec::new();
    into_writer(&value, &mut bytes).context("encode CBOR")?;
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<Value> {
    from_reader(bytes).context("decode CBOR")
}

pub fn int(value: i128) -> Value {
    if value >= 0 {
        Value::Integer(Integer::from(value as u64))
    } else {
        Value::Integer(Integer::from(value as i64))
    }
}

pub fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}

pub fn bytes(value: impl Into<Vec<u8>>) -> Value {
    Value::Bytes(value.into())
}

pub fn empty_map() -> Value {
    Value::Map(Vec::new())
}

pub fn as_array<'a>(value: &'a Value, label: &str) -> Result<&'a [Value]> {
    match value {
        Value::Array(items) => Ok(items),
        _ => bail!("{label} is not a CBOR array"),
    }
}

pub fn as_map<'a>(value: &'a Value, label: &str) -> Result<&'a [(Value, Value)]> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => bail!("{label} is not a CBOR map"),
    }
}

pub fn as_bytes<'a>(value: &'a Value, label: &str) -> Result<&'a [u8]> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        _ => bail!("{label} is not CBOR bytes"),
    }
}

pub fn as_text<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    match value {
        Value::Text(text) => Ok(text),
        _ => bail!("{label} is not CBOR text"),
    }
}

pub fn as_i128(value: &Value, label: &str) -> Result<i128> {
    match value {
        Value::Integer(number) => TryInto::<i128>::try_into(*number)
            .with_context(|| format!("convert {label} to integer")),
        _ => bail!("{label} is not a CBOR integer"),
    }
}

pub fn map_get<'a>(entries: &'a [(Value, Value)], key: i128) -> Option<&'a Value> {
    entries
        .iter()
        .find(|(candidate, _)| matches!(candidate, Value::Integer(value) if i128::try_from(*value).ok() == Some(key)))
        .map(|(_, value)| value)
}

pub fn map_get_text<'a>(entries: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|(candidate, _)| matches!(candidate, Value::Text(value) if value == key))
        .map(|(_, value)| value)
}

fn canonicalize(value: &Value) -> Result<Value> {
    match value {
        Value::Array(items) => Ok(Value::Array(
            items.iter().map(canonicalize).collect::<Result<Vec<_>>>()?,
        )),
        Value::Map(entries) => {
            let mut sorted = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let key = canonicalize(key)?;
                let value = canonicalize(value)?;
                let key_bytes = encode_preserving_order(&key)?;
                sorted.push((key_bytes, key, value));
            }

            sorted.sort_by(|left, right| {
                left.0
                    .len()
                    .cmp(&right.0.len())
                    .then_with(|| left.0.cmp(&right.0))
            });

            Ok(Value::Map(
                sorted
                    .into_iter()
                    .map(|(_, key, value)| (key, value))
                    .collect(),
            ))
        }
        Value::Tag(tag, inner) => Ok(Value::Tag(*tag, Box::new(canonicalize(inner)?))),
        other => Ok(other.clone()),
    }
}

fn encode_preserving_order(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    into_writer(value, &mut bytes).context("encode CBOR")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, int, text};
    use ciborium::value::Value;

    #[test]
    fn encode_sorts_map_keys_canonically() {
        let value = Value::Map(vec![
            (text("manufacturer"), text("google")),
            (text("brand"), text("google")),
            (text("device"), text("husky")),
        ]);

        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();
        let Value::Map(entries) = decoded else {
            panic!("encoded device info must decode as a map");
        };

        let keys = entries
            .into_iter()
            .map(|(key, _)| match key {
                Value::Text(key) => key,
                other => panic!("unexpected key type: {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["brand", "device", "manufacturer"]);
    }

    #[test]
    fn encode_sorts_integer_map_keys_canonically() {
        let value = Value::Map(vec![
            (int(-1), text("minus-one")),
            (int(3), text("three")),
            (int(1), text("one")),
        ]);

        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();
        let Value::Map(entries) = decoded else {
            panic!("encoded map must decode as a map");
        };

        let keys = entries.into_iter().map(|(key, _)| key).collect::<Vec<_>>();

        assert_eq!(keys, vec![int(1), int(3), int(-1)]);
    }
}
