use super::Curve;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeTuple};

// AE uses bincode's legacy fixed-width tuple layout. Version 1 has exactly three
// fields; reading new fields with serde(default) would still fail at its EOF.
impl Serialize for Curve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(if self.version == 1 { 3 } else { 5 })?;
        tuple.serialize_element(&self.version)?;
        tuple.serialize_element(&self.count)?;
        tuple.serialize_element(&self.points)?;
        if self.version != 1 {
            tuple.serialize_element(&self.manual)?;
            tuple.serialize_element(&self.handles)?;
        }
        tuple.end()
    }
}

impl<'de> Deserialize<'de> for Curve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Curve;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a version 1 or 2 ImageBevel profile")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Curve, A::Error> {
                let mut curve = Curve {
                    version: seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(0, &self))?,
                    count: seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(1, &self))?,
                    points: seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(2, &self))?,
                    manual: 0,
                    handles: [[[0.0; 2]; 2]; super::MAX_POINTS],
                };
                match curve.version {
                    1 => {}
                    2 => {
                        curve.manual = seq
                            .next_element()?
                            .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                        curve.handles = seq
                            .next_element()?
                            .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                    }
                    _ => return Err(de::Error::custom("unsupported ImageBevel profile version")),
                }
                Ok(curve.sanitized())
            }
        }
        deserializer.deserialize_tuple(5, Visitor)
    }
}
