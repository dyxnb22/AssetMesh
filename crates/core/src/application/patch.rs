//! Optional-field updates: omission preserves, null clears, a value replaces.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<T> {
    #[default]
    Leave,
    Set(T),
    Clear,
}

impl<T> Patch<T> {
    pub fn is_leave(&self) -> bool {
        matches!(self, Self::Leave)
    }

    pub fn apply_to(&self, field: &mut Option<T>)
    where
        T: Clone,
    {
        match self {
            Self::Leave => {}
            Self::Set(value) => *field = Some(value.clone()),
            Self::Clear => *field = None,
        }
    }

    pub fn try_map<U, E>(self, convert: impl FnOnce(T) -> Result<U, E>) -> Result<Patch<U>, E> {
        Ok(match self {
            Self::Leave => Patch::Leave,
            Self::Clear => Patch::Clear,
            Self::Set(value) => Patch::Set(convert(value)?),
        })
    }
}

impl<T> From<Option<T>> for Patch<T> {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Leave, Self::Set)
    }
}

impl Patch<String> {
    /// Empty text remains a supported clear for CLI and older desktop callers.
    pub fn from_text(value: Option<String>) -> Self {
        Self::from(value).normalize_text()
    }

    pub fn normalize_text(self) -> Self {
        match self {
            Self::Set(value) if value.trim().is_empty() => Self::Clear,
            other => other,
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map_or(Self::Clear, Self::Set))
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Set(value) => value.serialize(serializer),
            Self::Clear => serializer.serialize_none(),
            // Fields must use default + skip_serializing_if = "Patch::is_leave".
            // Refuse to turn a forgotten omission into a destructive clear.
            Self::Leave => Err(serde::ser::Error::custom(
                "an unchanged patch must be omitted",
            )),
        }
    }
}
