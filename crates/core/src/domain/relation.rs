//! Shared relations between assets (docs/03, ADR 0005).
//!
//! Relations connect assets through shared Asset identities — never through
//! module-private tables. A small registry defines each relation type's
//! inverse/symmetric semantics so one canonical row represents both
//! directions; nothing stores duplicate inverse rows.

use crate::domain::ids::{AssetId, RelationId};
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// Known relation types. The registry is intentionally tiny: only types a
/// real use case needs are added (docs/03 "Do not create dozens of relation
/// types before real use cases require them").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    DependsOn,
    DependencyOf,
    Uses,
    UsedBy,
    InstalledVia,
    Installs,
    RelatedTo,
}

impl RelationType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RelationType::DependsOn => "depends_on",
            RelationType::DependencyOf => "dependency_of",
            RelationType::Uses => "uses",
            RelationType::UsedBy => "used_by",
            RelationType::InstalledVia => "installed_via",
            RelationType::Installs => "installs",
            RelationType::RelatedTo => "related_to",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "depends_on" => Some(RelationType::DependsOn),
            "dependency_of" => Some(RelationType::DependencyOf),
            "uses" => Some(RelationType::Uses),
            "used_by" => Some(RelationType::UsedBy),
            "installed_via" => Some(RelationType::InstalledVia),
            "installs" => Some(RelationType::Installs),
            "related_to" => Some(RelationType::RelatedTo),
            _ => None,
        }
    }

    /// The inverse type, per the registry. A stored row
    /// `A --type--> B` viewed from B reads as `B --inverse--> A`.
    pub const fn inverse(&self) -> RelationType {
        match self {
            RelationType::DependsOn => RelationType::DependencyOf,
            RelationType::DependencyOf => RelationType::DependsOn,
            RelationType::Uses => RelationType::UsedBy,
            RelationType::UsedBy => RelationType::Uses,
            RelationType::InstalledVia => RelationType::Installs,
            RelationType::Installs => RelationType::InstalledVia,
            RelationType::RelatedTo => RelationType::RelatedTo,
        }
    }

    /// The canonical storage representative of an inverse pair. Inverse
    /// types (`dependency_of`, `used_by`, `installs`) are view-time
    /// derivations of their primary (`depends_on`, `uses`, `installed_via`)
    /// and are never stored: a fact stated with an inverse type is stored as
    /// its primary with the endpoints swapped, so every fact has exactly one
    /// row representation.
    pub const fn primary(&self) -> RelationType {
        match self {
            RelationType::DependencyOf => RelationType::DependsOn,
            RelationType::UsedBy => RelationType::Uses,
            RelationType::Installs => RelationType::InstalledVia,
            other => *other,
        }
    }

    /// Symmetric types read identically from both endpoints; canonical
    /// storage normalizes the endpoint order so `(A, B)` and `(B, A)` cannot
    /// coexist as duplicates.
    pub const fn is_symmetric(&self) -> bool {
        matches!(self, RelationType::RelatedTo)
    }

    /// How a stored relation reads when viewed from `viewer`. The source
    /// endpoint reads the stored type; the target endpoint reads the inverse.
    pub fn effective_from(&self, stored_source: AssetId, viewer: AssetId) -> RelationType {
        if viewer == stored_source {
            *self
        } else {
            self.inverse()
        }
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a relation came from. Discovered relations are suggestions until
/// confirmed; confirmed relations are canonical (docs/03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationProvenance {
    Manual,
    Discovered,
    Imported,
}

impl RelationProvenance {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RelationProvenance::Manual => "manual",
            RelationProvenance::Discovered => "discovered",
            RelationProvenance::Imported => "imported",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(RelationProvenance::Manual),
            "discovered" => Some(RelationProvenance::Discovered),
            "imported" => Some(RelationProvenance::Imported),
            _ => None,
        }
    }
}

/// One canonical relation row between two shared assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub id: RelationId,
    pub source_asset_id: AssetId,
    pub target_asset_id: AssetId,
    pub relation_type: RelationType,
    pub note: Option<String>,
    pub provenance: RelationProvenance,
    pub created_at: Timestamp,
}

impl Relation {
    /// Builds one relation. The id is generated by the application layer
    /// (docs/04); canonical storage form is applied separately via
    /// [`Relation::canonical_form`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RelationId,
        source_asset_id: AssetId,
        target_asset_id: AssetId,
        relation_type: RelationType,
        provenance: RelationProvenance,
        now: Timestamp,
    ) -> AppResult<Self> {
        let relation = Relation {
            id,
            source_asset_id,
            target_asset_id,
            relation_type,
            note: None,
            provenance,
            created_at: now,
        };
        relation.validate()?;
        Ok(relation)
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.source_asset_id == self.target_asset_id {
            return Err(AppError::validation(
                "a relation cannot connect an asset to itself",
            ));
        }
        Ok(())
    }

    /// True when this relation is already in canonical storage form. Every
    /// write path that reaches a repository must store a canonical row, so
    /// the storage-level guarantees (one row per fact) hold even for direct
    /// UnitOfWork writes that skip the application services.
    pub fn is_canonical(&self) -> bool {
        if self.relation_type != self.relation_type.primary() {
            return false;
        }
        if self.relation_type.is_symmetric()
            && self.target_asset_id.as_uuid() < self.source_asset_id.as_uuid()
        {
            return false;
        }
        true
    }

    /// Rejects anything a repository would have to rewrite to store. Callers
    /// that accept a user-stated fact in either direction should apply
    /// [`Relation::canonical_form`] first; this is the boundary check that
    /// keeps the "exactly one row per fact" invariant unbroken at the
    /// repository seam.
    pub fn ensure_canonical(&self) -> AppResult<()> {
        if self.relation_type != self.relation_type.primary() {
            return Err(AppError::validation(format!(
                "{} is the view-time inverse of {}; store the fact as {} from {} to {}",
                self.relation_type,
                self.relation_type.primary(),
                self.relation_type.primary(),
                self.target_asset_id,
                self.source_asset_id
            )));
        }
        if self.relation_type.is_symmetric()
            && self.target_asset_id.as_uuid() < self.source_asset_id.as_uuid()
        {
            return Err(AppError::validation(format!(
                "{} is symmetric and must store the lexicographically smaller asset id \
                 as source; use {} -> {} instead of {} -> {}",
                self.relation_type,
                self.target_asset_id,
                self.source_asset_id,
                self.source_asset_id,
                self.target_asset_id
            )));
        }
        Ok(())
    }

    /// Canonical storage form, applied by every write path (service attach,
    /// merge re-pointing, portable import). Two canonical rows represent the
    /// same fact if and only if their `(source, target, relation_type)`
    /// triples are equal:
    ///
    /// - inverse-pair types collapse onto their [`RelationType::primary`]
    ///   with the endpoints swapped (`B -dependency_of-> A` becomes
    ///   `A -depends_on-> B`);
    /// - symmetric types store the lexicographically smaller asset id as
    ///   source.
    pub fn canonical_form(mut self) -> Self {
        let primary = self.relation_type.primary();
        if primary != self.relation_type {
            std::mem::swap(&mut self.source_asset_id, &mut self.target_asset_id);
            self.relation_type = primary;
        }
        if self.relation_type.is_symmetric()
            && self.target_asset_id.as_uuid() < self.source_asset_id.as_uuid()
        {
            std::mem::swap(&mut self.source_asset_id, &mut self.target_asset_id);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_registry_is_symmetrically_consistent() {
        for relation_type in [
            RelationType::DependsOn,
            RelationType::DependencyOf,
            RelationType::Uses,
            RelationType::UsedBy,
            RelationType::InstalledVia,
            RelationType::Installs,
            RelationType::RelatedTo,
        ] {
            let inverse = relation_type.inverse();
            assert_eq!(
                inverse.inverse(),
                relation_type,
                "inverse of inverse must round-trip for {relation_type}"
            );
            assert_eq!(
                inverse.is_symmetric(),
                relation_type.is_symmetric(),
                "symmetry must agree with the inverse for {relation_type}"
            );
        }
        assert!(RelationType::RelatedTo.is_symmetric());
        assert!(!RelationType::DependsOn.is_symmetric());
    }

    #[test]
    fn effective_type_reads_correctly_from_each_endpoint() {
        let a = AssetId::generate();
        let b = AssetId::generate();
        assert_eq!(
            RelationType::DependsOn.effective_from(a, a),
            RelationType::DependsOn
        );
        assert_eq!(
            RelationType::DependsOn.effective_from(a, b),
            RelationType::DependencyOf
        );
        assert_eq!(
            RelationType::RelatedTo.effective_from(a, b),
            RelationType::RelatedTo
        );
    }

    #[test]
    fn self_relations_are_invalid() {
        let id = AssetId::generate();
        let relation = Relation::new(
            RelationId::generate(),
            id,
            id,
            RelationType::Uses,
            RelationProvenance::Manual,
            chrono::Utc::now(),
        );
        assert!(relation.is_err());
    }

    #[test]
    fn canonical_form_gives_every_fact_exactly_one_representation() {
        let a = AssetId::from_uuid(uuid::Uuid::from_u128(1));
        let b = AssetId::from_uuid(uuid::Uuid::from_u128(2));
        let now = chrono::Utc::now();

        // Symmetric: endpoint order normalized.
        let related = Relation::new(
            RelationId::generate(),
            b,
            a,
            RelationType::RelatedTo,
            RelationProvenance::Manual,
            now,
        )
        .unwrap()
        .canonical_form();
        assert_eq!(related.source_asset_id, a);
        assert_eq!(related.target_asset_id, b);
        assert_eq!(related.relation_type, RelationType::RelatedTo);

        // Inverse pair: both statements of the same fact collapse onto the
        // primary direction with identical triples.
        let direct = Relation::new(
            RelationId::generate(),
            a,
            b,
            RelationType::DependsOn,
            RelationProvenance::Manual,
            now,
        )
        .unwrap()
        .canonical_form();
        let stated_via_inverse = Relation::new(
            RelationId::generate(),
            b,
            a,
            RelationType::DependencyOf,
            RelationProvenance::Manual,
            now,
        )
        .unwrap()
        .canonical_form();
        assert_eq!(direct.source_asset_id, stated_via_inverse.source_asset_id);
        assert_eq!(direct.target_asset_id, stated_via_inverse.target_asset_id);
        assert_eq!(direct.relation_type, stated_via_inverse.relation_type);
        assert_eq!(direct.relation_type, RelationType::DependsOn);

        // The opposite direction is a DIFFERENT fact and stays distinct.
        let opposite = Relation::new(
            RelationId::generate(),
            b,
            a,
            RelationType::DependsOn,
            RelationProvenance::Manual,
            now,
        )
        .unwrap()
        .canonical_form();
        assert_eq!(opposite.source_asset_id, b);
        assert_eq!(opposite.relation_type, RelationType::DependsOn);

        // Primary types are already canonical.
        for relation_type in [
            RelationType::DependsOn,
            RelationType::Uses,
            RelationType::InstalledVia,
            RelationType::RelatedTo,
        ] {
            let relation = Relation::new(
                RelationId::generate(),
                a,
                b,
                relation_type,
                RelationProvenance::Manual,
                now,
            )
            .unwrap()
            .canonical_form();
            assert_eq!(relation.relation_type, relation_type);
        }
    }

    #[test]
    fn primary_collapses_all_inverse_types() {
        assert_eq!(
            RelationType::DependencyOf.primary(),
            RelationType::DependsOn
        );
        assert_eq!(RelationType::UsedBy.primary(), RelationType::Uses);
        assert_eq!(RelationType::Installs.primary(), RelationType::InstalledVia);
        assert_eq!(RelationType::RelatedTo.primary(), RelationType::RelatedTo);
    }

    #[test]
    fn canonical_form_recognized_and_enforced() {
        let a = AssetId::from_uuid(uuid::Uuid::from_u128(1));
        let b = AssetId::from_uuid(uuid::Uuid::from_u128(2));
        let now = chrono::Utc::now();

        let canonical = Relation::new(
            RelationId::generate(),
            a,
            b,
            RelationType::RelatedTo,
            RelationProvenance::Manual,
            now,
        )
        .unwrap();
        assert!(canonical.is_canonical());
        assert!(canonical.ensure_canonical().is_ok());

        // Reversed symmetric endpoints are not canonical.
        let reversed = Relation::new(
            RelationId::generate(),
            b,
            a,
            RelationType::RelatedTo,
            RelationProvenance::Manual,
            now,
        )
        .unwrap();
        assert!(!reversed.is_canonical());
        assert!(reversed.ensure_canonical().is_err());

        // Inverse types are never canonical, in either direction.
        for inverse in [
            RelationType::DependencyOf,
            RelationType::UsedBy,
            RelationType::Installs,
        ] {
            let stated = Relation::new(
                RelationId::generate(),
                b,
                a,
                inverse,
                RelationProvenance::Manual,
                now,
            )
            .unwrap();
            assert!(!stated.is_canonical(), "{inverse} must not be canonical");
            assert!(stated.ensure_canonical().is_err());
        }

        // A primary asymmetric fact in its own direction is canonical.
        let direct = Relation::new(
            RelationId::generate(),
            a,
            b,
            RelationType::DependsOn,
            RelationProvenance::Manual,
            now,
        )
        .unwrap();
        assert!(direct.is_canonical());
    }

    #[test]
    fn type_parsing() {
        assert_eq!(
            RelationType::parse("depends_on"),
            Some(RelationType::DependsOn)
        );
        assert_eq!(
            RelationType::parse("RELATED_TO"),
            Some(RelationType::RelatedTo)
        );
        assert_eq!(RelationType::parse("owns"), None);
        assert_eq!(
            RelationProvenance::parse("imported"),
            Some(RelationProvenance::Imported)
        );
    }
}
