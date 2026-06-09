use serde::{Deserialize, Deserializer};

pub fn nullable<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(de)?;
    Ok(s.and_then(|v| if v.trim().is_empty() { None } else { Some(v) }))
}
