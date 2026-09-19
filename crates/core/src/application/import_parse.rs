//! Legacy import format adapters.
//!
//! Legacy-format-specific parsing is isolated here (docs/08 import
//! contract). Parsers translate JSON/CSV rows into one neutral
//! [`RawMediaInput`] shape; the import pipeline never sees the original
//! format. Adding another legacy format means adding a parser here, not
//! changing the pipeline.
//!
//! Parsing is strict: a non-numeric value in a numeric column, or a
//! malformed external-ref token, visibly rejects the row instead of
//! silently disappearing.

use crate::domain::external_ref::{validate_external_id, validate_namespace};
use crate::domain::ids::AssetId;
use crate::domain::media::{MediaStatus, MediaType, Progress};
use crate::domain::Timestamp;
use crate::AppResult;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Format-neutral parsed row. All fields optional; validation happens in the
/// normalize step of the pipeline. Unknown keys are ignored for lenience.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawMediaInput {
    #[serde(alias = "name")]
    pub title: Option<String>,
    #[serde(alias = "type", alias = "kind")]
    pub media_type: Option<String>,
    pub status: Option<String>,
    pub rating: Option<f64>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub progress_current: Option<f64>,
    pub progress_total: Option<f64>,
    pub progress_unit: Option<String>,
    pub notes: Option<String>,
    pub summary: Option<String>,
    pub tags: Option<Vec<String>>,
    pub external_refs: Option<RawExternalRefs>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    /// Canonical AssetMesh ID — used when re-importing AssetMesh-native data.
    pub asset_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawExternalRefs {
    /// `{"steam": "1091500", "igdb": "1877"}`
    Map(BTreeMap<String, String>),
    /// `[{"namespace": "steam", "external_id": "1091500"}]`
    List(Vec<RawExternalRef>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawExternalRef {
    #[serde(alias = "ns")]
    pub namespace: String,
    #[serde(alias = "id", alias = "ext_id")]
    pub external_id: String,
    pub source_url: Option<String>,
}

/// An ownerless external reference: identity fields only, re-pointed at the
/// matched/created asset at commit time. Ownerless refs keep planning and
/// commit symmetric — no placeholder IDs can leak into a foreign-key check.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingExternalRef {
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
}

impl PendingExternalRef {
    pub fn key(&self) -> (&str, &str) {
        (&self.namespace, &self.external_id)
    }

    pub fn validate(&self) -> crate::AppResult<()> {
        validate_namespace(&self.namespace)?;
        validate_external_id(&self.external_id)?;
        Ok(())
    }
}

/// A validated, normalized candidate ready for matching.
#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub index: usize,
    pub asset_id: Option<AssetId>,
    pub title: String,
    pub media_type: MediaType,
    pub status: Option<MediaStatus>,
    pub rating: Option<f64>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub progress: Progress,
    pub notes: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub refs: Vec<PendingExternalRef>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub warnings: Vec<String>,
    /// Asset ID pre-assigned when this candidate is planned as a create, so
    /// later batch records can match against it before anything is committed.
    pub planned_asset_id: Option<AssetId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFormat {
    Json,
    Csv,
}

/// Sniffs the format when not given explicitly.
pub fn detect_format(text: &str, hint: Option<&str>) -> ImportFormat {
    match hint {
        Some(ext) if ext.eq_ignore_ascii_case("csv") => return ImportFormat::Csv,
        Some(ext) if ext.eq_ignore_ascii_case("json") => return ImportFormat::Json,
        _ => {}
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        ImportFormat::Json
    } else {
        ImportFormat::Csv
    }
}

/// Outcome of parsing one physical row. Both variants carry the physical
/// (zero-based) source index so reports never drift when malformed rows
/// precede valid ones.
#[derive(Debug, Clone)]
pub enum ParsedRow {
    Raw {
        index: usize,
        raw: Box<RawMediaInput>,
    },
    Malformed {
        index: usize,
        reason: String,
    },
}

/// Parses a JSON document containing an array of records (or an object with
/// a `records`, `items`, or `data` array).
pub fn parse_json(text: &str) -> AppResult<Vec<ParsedRow>> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| crate::AppError::validation(format!("invalid JSON: {e}")))?;

    let records = match &value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(map) => {
            let key = ["records", "items", "data"]
                .into_iter()
                .find(|k| map.contains_key(*k))
                .ok_or_else(|| {
                    crate::AppError::validation(
                        "JSON object must contain a \"records\", \"items\", or \"data\" array",
                    )
                })?;
            map[key]
                .as_array()
                .cloned()
                .ok_or_else(|| crate::AppError::validation("records must be a JSON array"))?
        }
        _ => {
            return Err(crate::AppError::validation(
                "JSON import must be an array of records",
            ))
        }
    };

    Ok(records
        .into_iter()
        .enumerate()
        .map(
            |(index, item)| match serde_json::from_value::<RawMediaInput>(item) {
                Ok(raw) => ParsedRow::Raw {
                    index,
                    raw: Box::new(raw),
                },
                Err(e) => ParsedRow::Malformed {
                    index,
                    reason: format!("record {index} is malformed: {e}"),
                },
            },
        )
        .collect())
}

/// Parses CSV rows. Expected headers (case-insensitive): title, media_type,
/// status, rating, year, platform, progress_current, progress_total,
/// progress_unit, notes, summary, tags (semicolon-separated), external_refs
/// (semicolon-separated `namespace:external_id` pairs), started_at,
/// completed_at, asset_id.
pub fn parse_csv(text: &str) -> AppResult<Vec<ParsedRow>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = {
        let headers = reader
            .headers()
            .map_err(|e| crate::AppError::validation(format!("invalid CSV header: {e}")))?;
        headers
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect()
    };

    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = match record {
            Ok(record) => record,
            Err(e) => {
                rows.push(ParsedRow::Malformed {
                    index,
                    reason: format!("record {index} is malformed: {e}"),
                });
                continue;
            }
        };

        let get = |name: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|i| record.get(i))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        // Numeric columns reject non-numeric values instead of dropping
        // them; the first problem is reported for the row.
        let mut malformed: Option<String> = None;
        let mut parse_number = |name: &'static str| -> Option<f64> {
            match get(name) {
                None => None,
                Some(v) => match v.parse::<f64>() {
                    Ok(n) => Some(n),
                    Err(_) => {
                        if malformed.is_none() {
                            malformed =
                                Some(format!("record {index} has a non-numeric {name}: {v}"));
                        }
                        None
                    }
                },
            }
        };

        let rating = parse_number("rating");
        let progress_current = parse_number("progress_current");
        let progress_total = parse_number("progress_total");
        let year = match get("year") {
            None => None,
            Some(v) => match v.parse::<i32>() {
                Ok(y) => Some(y),
                Err(_) => {
                    if malformed.is_none() {
                        malformed = Some(format!("record {index} has a non-numeric year: {v}"));
                    }
                    None
                }
            },
        };

        // Malformed external-ref tokens are rejections, not silent drops.
        let mut refs: Vec<RawExternalRef> = Vec::new();
        if let Some(v) = get("external_refs") {
            for token in v.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                match parse_ref_token(token) {
                    Some(r) => refs.push(r),
                    None => {
                        malformed = Some(format!(
                            "record {index} has a malformed external ref {token:?} (expected namespace:external_id)"
                        ));
                    }
                }
            }
        }

        if let Some(reason) = malformed {
            rows.push(ParsedRow::Malformed { index, reason });
            continue;
        }
        let _ = index;

        let raw = RawMediaInput {
            title: get("title").or_else(|| get("name")),
            media_type: get("media_type").or_else(|| get("type")),
            status: get("status"),
            rating,
            year,
            platform: get("platform"),
            progress_current,
            progress_total,
            progress_unit: get("progress_unit"),
            notes: get("notes"),
            summary: get("summary"),
            tags: get("tags").map(|v| {
                v.split([';', '|'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }),
            external_refs: (!refs.is_empty()).then_some(RawExternalRefs::List(refs)),
            started_at: get("started_at"),
            completed_at: get("completed_at"),
            asset_id: get("asset_id"),
        };

        rows.push(ParsedRow::Raw {
            index,
            raw: Box::new(raw),
        });
    }
    Ok(rows)
}

/// `namespace:external_id` — external_id may itself contain colons.
fn parse_ref_token(token: &str) -> Option<RawExternalRef> {
    let (namespace, external_id) = token.split_once(':')?;
    Some(RawExternalRef {
        namespace: namespace.trim().to_string(),
        external_id: external_id.trim().to_string(),
        source_url: None,
    })
}

/// Parses a timestamp in RFC 3339 or plain `YYYY-MM-DD` form.
pub fn parse_timestamp(value: &str) -> Option<Timestamp> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(ts.with_timezone(&chrono::Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let naive = date.and_hms_opt(0, 0, 0)?;
        return Some(naive.and_utc());
    }
    None
}

/// Case-folds, trims, and collapses whitespace for deterministic matching.
pub fn normalize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;
    for c in title.trim().chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            for lowered in c.to_lowercase() {
                out.push(lowered);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parsing_accepts_array_and_wrapper() {
        let rows = parse_json(r#"[{"title":"Frieren","media_type":"anime"}]"#).unwrap();
        assert_eq!(rows.len(), 1);

        let rows = parse_json(
            r#"{"records":[{"title":"Frieren","media_type":"anime"},{"title":"oops"}]}"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn json_parsing_rejects_bad_records() {
        let rows = parse_json(r#"[{"title":123}]"#).unwrap();
        match &rows[0] {
            ParsedRow::Malformed { reason, .. } => assert!(reason.contains("malformed")),
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn csv_parsing_maps_headers() {
        let csv_text = "title,media_type,status,year,rating,tags,external_refs\n\
                        Frieren,anime,completed,2023,9.5,healing;fantasy,tmdb:209867\n";
        let rows = parse_csv(csv_text).unwrap();
        match &rows[0] {
            ParsedRow::Raw { raw, .. } => {
                assert_eq!(raw.title.as_deref(), Some("Frieren"));
                assert_eq!(raw.media_type.as_deref(), Some("anime"));
                assert_eq!(raw.tags.as_ref().map(|t| t.len()), Some(2));
                assert_eq!(
                    raw.external_refs.as_ref().map(|r| match r {
                        RawExternalRefs::List(items) => items.len(),
                        RawExternalRefs::Map(map) => map.len(),
                    }),
                    Some(1)
                );
            }
            other => panic!("expected raw row, got {other:?}"),
        }
    }

    #[test]
    fn csv_non_numeric_rating_is_rejected() {
        let csv_text = "title,media_type,rating\nFrieren,anime,excellent\n";
        let rows = parse_csv(csv_text).unwrap();
        assert!(matches!(rows[0], ParsedRow::Malformed { .. }));
    }

    #[test]
    fn csv_non_numeric_progress_is_rejected() {
        let csv_text = "title,media_type,progress_current\nFrieren,anime,several\n";
        let rows = parse_csv(csv_text).unwrap();
        match &rows[0] {
            ParsedRow::Malformed { reason, .. } => assert!(reason.contains("progress_current")),
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn csv_malformed_ref_token_is_rejected() {
        let csv_text = "title,media_type,external_refs\nFrieren,anime,tmdb-no-colon\n";
        let rows = parse_csv(csv_text).unwrap();
        match &rows[0] {
            ParsedRow::Malformed { reason, .. } => assert!(reason.contains("external ref")),
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn normalize_title_collapses_case_and_space() {
        assert_eq!(
            normalize_title("  Sousou   no FRIEREN "),
            "sousou no frieren"
        );
        assert_eq!(normalize_title(""), "");
    }

    #[test]
    fn timestamp_parsing() {
        assert!(parse_timestamp("2024-01-05").is_some());
        assert!(parse_timestamp("2024-01-05T10:00:00Z").is_some());
        assert!(parse_timestamp("yesterday").is_none());
    }

    #[test]
    fn ref_token_parsing() {
        let r = parse_ref_token("steam:1091500").unwrap();
        assert_eq!(r.namespace, "steam");
        assert_eq!(r.external_id, "1091500");
        // external ids may contain colons
        let r = parse_ref_token("url:https://example.com/a:b").unwrap();
        assert_eq!(r.external_id, "https://example.com/a:b");
        assert!(parse_ref_token("nocolon").is_none());
    }
}
