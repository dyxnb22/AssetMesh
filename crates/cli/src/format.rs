//! Human-readable output formatting for the CLI. No business logic here.

use assetmesh_core::application::activity_service::ActivityView;
use assetmesh_core::application::duplicate_review_service::DuplicateCandidate;
use assetmesh_core::application::import_media::ImportReport;
use assetmesh_core::application::library_service::{
    AssetDetailView, AssetDetails, AssetSummary, Page,
};
use assetmesh_core::application::media_service::MediaView;
use assetmesh_core::application::relation_query_service::{NeighborView, TraversalView};
use assetmesh_core::domain::activity::ActivityEvent;
use assetmesh_core::domain::external_ref::AssetExternalRef;
use assetmesh_core::domain::media::{MediaRecord, Progress};
use assetmesh_core::domain::search::SearchHit;
use assetmesh_core::domain::service::ServiceRecord;
use assetmesh_core::domain::software::SoftwareRecord;
use assetmesh_core::ports::repos::MediaListRow;

fn fmt_progress(progress: &Progress) -> String {
    match (&progress.current, &progress.total, &progress.unit) {
        (Some(c), Some(t), Some(u)) => format!("{c}/{t} {u}"),
        (Some(c), None, Some(u)) => format!("{c} {u}"),
        (None, Some(t), Some(u)) => format!("?/{t} {u}"),
        (Some(c), Some(t), None) => format!("{c}/{t}"),
        (Some(c), None, None) => format!("{c}"),
        _ => "-".to_string(),
    }
}

fn fmt_rating(rating: Option<f64>) -> String {
    rating
        .map(|r| format!("{r:.1}"))
        .unwrap_or_else(|| "-".into())
}

pub(crate) fn fmt_time(ts: Option<chrono::DateTime<chrono::Utc>>) -> String {
    ts.map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".into())
}

/// Tags, external references, and activity are shared by every detail view, so
/// the module printers and the unified printer share these helpers.
fn print_tags(label: &str, tags: &[String]) {
    if !tags.is_empty() {
        println!("{label}{}", tags.join(", "));
    }
}

fn print_refs(refs: &[AssetExternalRef]) {
    if refs.is_empty() {
        return;
    }
    println!("Refs:");
    for reference in refs {
        println!(
            "  {}:{}{}",
            reference.namespace,
            reference.external_id,
            reference
                .source_url
                .as_deref()
                .map(|u| format!(" ({u})"))
                .unwrap_or_default()
        );
    }
}

fn print_activity(activity: &[ActivityEvent]) {
    if activity.is_empty() {
        return;
    }
    println!("Activity:");
    for event in activity {
        println!(
            "  {}  {} [{}]  {}",
            event.occurred_at.format("%Y-%m-%d %H:%M"),
            event.event_type,
            event.actor,
            event.payload
        );
    }
}

/// Media-owned detail fields. Shared by `media get` and the unified
/// `library get`, so a module field is never formatted twice.
fn print_media_record_fields(record: &MediaRecord) {
    println!("Type:     {}", record.media_type);
    println!("Status:   {}", record.status);
    println!("Rating:   {}", fmt_rating(record.rating));
    println!(
        "Year:     {}",
        record
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!("Platform: {}", record.platform.as_deref().unwrap_or("-"));
    println!("Progress: {}", fmt_progress(&record.progress));
    println!("Started:  {}", fmt_time(record.started_at));
    println!("Completed: {}", fmt_time(record.completed_at));
    if let Some(notes) = &record.notes {
        println!("Notes:    {notes}");
    }
}

fn print_software_record_fields(record: &SoftwareRecord) {
    println!("Category:       {}", record.category);
    println!("Install source: {}", record.install_source);
    println!(
        "Version:        {}",
        record.version.as_deref().unwrap_or("-")
    );
    println!(
        "Location:       {}",
        record.install_location.as_deref().unwrap_or("-")
    );
    println!(
        "Executable:     {}",
        record.executable_path.as_deref().unwrap_or("-")
    );
    println!(
        "Architecture:   {}",
        record.architecture.as_deref().unwrap_or("-")
    );
    println!("Discovered:     {}", fmt_time(record.discovered_at));
    println!("Installed:      {}", fmt_time(record.installed_at));
    if let Some(purpose) = &record.purpose {
        println!("Purpose:        {purpose}");
    }
    if let Some(notes) = &record.notes {
        println!("Notes:          {notes}");
    }
}

fn print_service_record_fields(record: &ServiceRecord) {
    println!("Type:          {}", record.service_type);
    println!(
        "Provider:      {}",
        record.provider.as_deref().unwrap_or("-")
    );
    println!(
        "Account:       {}",
        record.account_label.as_deref().unwrap_or("-")
    );
    println!(
        "Endpoint:      {}",
        record.endpoint_url.as_deref().unwrap_or("-")
    );
    println!(
        "Dashboard:     {}",
        record.dashboard_url.as_deref().unwrap_or("-")
    );
    println!(
        "Domain:        {}",
        record.domain_name.as_deref().unwrap_or("-")
    );
    println!("Plan:          {}", record.plan.as_deref().unwrap_or("-"));
    match (record.cost_minor, record.currency.as_deref()) {
        (Some(cost), Some(currency)) => {
            println!(
                "Cost:          {}",
                assetmesh_core::domain::service::format_money(cost, currency)
            );
        }
        _ => println!("Cost:          -"),
    }
    println!(
        "Billing:       {}",
        record
            .billing_cadence
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!("Renews:        {}", fmt_time(record.renews_at));
    println!("Expires:       {}", fmt_time(record.expires_at));
    println!(
        "Auto-renew:    {}",
        match record.auto_renew {
            Some(true) => "on",
            Some(false) => "off",
            None => "unknown",
        }
    );
    if let Some(notes) = &record.notes {
        println!("Notes:         {notes}");
    }
}

pub fn print_media_detail(view: &MediaView) {
    let entry = &view.entry;
    println!("ID:       {}", entry.asset.id);
    println!("Kind:     {}", entry.asset.kind);
    println!("Title:    {}", entry.asset.name);
    if let Some(summary) = &entry.asset.summary {
        println!("Summary:  {summary}");
    }
    print_media_record_fields(&entry.record);
    print_tags("Tags:     ", &view.tags);
    print_refs(&view.external_refs);
    print_activity(&view.activity);
}

pub fn print_media_list(rows: &[MediaListRow], json: bool) {
    if json {
        let value: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.entry.asset.id.to_string(),
                    "kind": row.entry.asset.kind.as_str(),
                    "title": row.entry.asset.name,
                    "media_type": row.entry.record.media_type.as_str(),
                    "status": row.entry.record.status.as_str(),
                    "rating": row.entry.record.rating,
                    "year": row.entry.record.year,
                    "progress": {
                        "current": row.entry.record.progress.current,
                        "total": row.entry.record.progress.total,
                        "unit": row.entry.record.progress.unit,
                    },
                    "tags": row.tags,
                    "updated_at": row.entry.asset.updated_at.to_rfc3339(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "[]".to_string())
        );
        return;
    }

    if rows.is_empty() {
        println!("(no media records)");
        return;
    }
    println!(
        "{:38}  {:<28}  {:<6}  {:<11}  {:>5}  {:<16}  UPDATED",
        "ID", "TITLE", "TYPE", "STATUS", "RATE", "PROGRESS"
    );
    for row in rows {
        println!(
            "{:38}  {:<28}  {:<6}  {:<11}  {:>5}  {:<16}  {}",
            truncate(&row.entry.asset.id.to_string(), 38),
            truncate(&row.entry.asset.name, 28),
            row.entry.record.media_type,
            row.entry.record.status,
            fmt_rating(row.entry.record.rating),
            fmt_progress(&row.entry.record.progress),
            row.entry.asset.updated_at.format("%Y-%m-%d"),
        );
    }
    println!("\n{} record(s)", rows.len());
}

pub fn print_search_hits(hits: &[SearchHit]) {
    if hits.is_empty() {
        println!("(no results)");
        return;
    }
    for hit in hits {
        match &hit.subtitle {
            Some(subtitle) => println!(
                "{}  {} — {}",
                truncate(&hit.asset_id.to_string(), 38),
                hit.title,
                subtitle
            ),
            None => println!("{}  {}", truncate(&hit.asset_id.to_string(), 38), hit.title),
        }
    }
    println!("\n{} hit(s)", hits.len());
}

pub fn print_import_report(report: &ImportReport, dry_run: bool) {
    println!("Input:                {}", report.input);
    println!("Valid:                {}", report.valid);
    if dry_run {
        println!("Would create:         {}", report.create);
        println!("Would update:         {}", report.update);
    } else {
        println!("Created:              {}", report.create);
        println!("Updated:              {}", report.update);
        println!("Unchanged:            {}", report.unchanged);
    }
    println!("Potential duplicates: {}", report.potential_duplicates);
    println!("Rejected:             {}", report.rejected);

    for conflict in &report.conflicts {
        println!(
            "  conflict #{} {:?} → {} ({})",
            conflict.index,
            conflict.title,
            conflict
                .candidate_asset_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".into()),
            conflict.reason
        );
    }
    for rejected in &report.rejected_records {
        println!("  rejected #{}: {}", rejected.index, rejected.reason);
    }
}

fn truncate(value: &str, max: usize) -> String {
    // Char-boundary safe: byte slicing would panic on multi-byte UTF-8
    // (CJK titles, emoji). Width is not considered; alignment is best-effort.
    if value.chars().count() <= max {
        return value.to_string();
    }
    let head: String = value.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

use assetmesh_core::application::relation_service::RelationView;
use assetmesh_core::application::service_service::ServiceView;
use assetmesh_core::application::software_discovery::CandidateDisposition;
use assetmesh_core::application::software_service::{AdoptionOutcome, ScanReport, SoftwareView};
use assetmesh_core::ports::repos::{ServiceListRow, SoftwareListRow};

pub fn print_software_detail(view: &SoftwareView) {
    let entry = &view.entry;
    println!("ID:             {}", entry.asset.id);
    println!("Kind:           {}", entry.asset.kind);
    println!("Name:           {}", entry.asset.name);
    if let Some(summary) = &entry.asset.summary {
        println!("Summary:        {summary}");
    }
    print_software_record_fields(&entry.record);
    print_tags("Tags:           ", &view.tags);
    print_refs(&view.external_refs);
    print_activity(&view.activity);
}

pub fn print_software_list(rows: &[SoftwareListRow], json: bool) {
    if json {
        let value: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.entry.asset.id.to_string(),
                    "kind": row.entry.asset.kind.as_str(),
                    "name": row.entry.asset.name,
                    "category": row.entry.record.category.as_str(),
                    "install_source": row.entry.record.install_source.as_str(),
                    "version": row.entry.record.version,
                    "purpose": row.entry.record.purpose,
                    "tags": row.tags,
                    "updated_at": row.entry.asset.updated_at.to_rfc3339(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "[]".to_string())
        );
        return;
    }

    if rows.is_empty() {
        println!("(no software records)");
        return;
    }
    println!(
        "{:38}  {:<28}  {:<11}  {:<16}  {:<10}  UPDATED",
        "ID", "NAME", "CATEGORY", "SOURCE", "VERSION"
    );
    for row in rows {
        println!(
            "{:38}  {:<28}  {:<11}  {:<16}  {:<10}  {}",
            truncate(&row.entry.asset.id.to_string(), 38),
            truncate(&row.entry.asset.name, 28),
            row.entry.record.category,
            row.entry.record.install_source,
            truncate(row.entry.record.version.as_deref().unwrap_or("-"), 10),
            row.entry.asset.updated_at.format("%Y-%m-%d"),
        );
    }
    println!("\n{} record(s)", rows.len());
}

/// Discovery output: candidates are clearly NOT canonical assets.
pub fn print_scan_report(report: &ScanReport) {
    println!(
        "provider: {} — {} candidate(s) discovered (candidates are not canonical assets)",
        report.provider,
        report.candidates.len()
    );
    for classified in &report.candidates {
        let candidate = &classified.candidate;
        let refs = candidate
            .external_refs
            .iter()
            .map(|r| format!("{}:{}", r.namespace, r.external_id))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  [{}] {} ({}){} — refs: {}",
            classified.disposition.kind(),
            candidate.display_name,
            candidate.category,
            candidate
                .version
                .as_deref()
                .map(|v| format!(" {v}"))
                .unwrap_or_default(),
            if refs.is_empty() { "-" } else { &refs }
        );
        if let CandidateDisposition::PotentialDuplicate { asset_ids } = &classified.disposition {
            println!(
                "      review required — resembles {} asset(s): {}",
                asset_ids.len(),
                asset_ids
                    .iter()
                    .map(|id| truncate(&id.to_string(), 38))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let CandidateDisposition::Conflict { message } = &classified.disposition {
            println!("      conflict — {message}");
        }
    }
}

pub fn print_adoption_outcome(outcome: &AdoptionOutcome) {
    let action = if outcome.created {
        "created"
    } else {
        "updated"
    };
    println!(
        "adopted candidate as {} ({}, disposition: {})",
        outcome.asset_id, action, outcome.disposition
    );
    if !outcome.updated_fields.is_empty() {
        println!("  updated fields: {}", outcome.updated_fields.join(", "));
    }
    if !outcome.skipped_refs.is_empty() {
        println!("  skipped refs:   {}", outcome.skipped_refs.join(", "));
    }
}

pub fn print_service_detail(view: &ServiceView) {
    let entry = &view.entry;
    println!("ID:            {}", entry.asset.id);
    println!("Kind:          {}", entry.asset.kind);
    println!("Name:          {}", entry.asset.name);
    if let Some(summary) = &entry.asset.summary {
        println!("Summary:       {summary}");
    }
    print_service_record_fields(&entry.record);
    print_tags("Tags:          ", &view.tags);
    print_refs(&view.external_refs);
    print_activity(&view.activity);
}

pub fn print_service_list(rows: &[ServiceListRow], json: bool) {
    if json {
        let value: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.entry.asset.id.to_string(),
                    "kind": row.entry.asset.kind.as_str(),
                    "name": row.entry.asset.name,
                    "service_type": row.entry.record.service_type.as_str(),
                    "provider": row.entry.record.provider,
                    "plan": row.entry.record.plan,
                    "cost_minor": row.entry.record.cost_minor,
                    "currency": row.entry.record.currency,
                    "billing_cadence": row.entry.record.billing_cadence.map(|c| c.as_str()),
                    "renews_at": row.entry.record.renews_at.map(|t| t.to_rfc3339()),
                    "expires_at": row.entry.record.expires_at.map(|t| t.to_rfc3339()),
                    "auto_renew": row.entry.record.auto_renew,
                    "tags": row.tags,
                    "updated_at": row.entry.asset.updated_at.to_rfc3339(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "[]".to_string())
        );
        return;
    }

    if rows.is_empty() {
        println!("(no service records)");
        return;
    }
    println!(
        "{:38}  {:<28}  {:<7}  {:<16}  {:<16}  UPDATED",
        "ID", "NAME", "TYPE", "PROVIDER", "PLAN"
    );
    for row in rows {
        println!(
            "{:38}  {:<28}  {:<7}  {:<16}  {:<16}  {}",
            truncate(&row.entry.asset.id.to_string(), 38),
            truncate(&row.entry.asset.name, 28),
            row.entry.record.service_type,
            truncate(row.entry.record.provider.as_deref().unwrap_or("-"), 16),
            truncate(row.entry.record.plan.as_deref().unwrap_or("-"), 16),
            row.entry.asset.updated_at.format("%Y-%m-%d"),
        );
    }
    println!("\n{} record(s)", rows.len());
}

pub fn print_relation_views(views: &[RelationView]) {
    if views.is_empty() {
        println!("(no relations)");
        return;
    }
    for view in views {
        println!(
            "{}  {}  {}  {}{}",
            truncate(&view.relation_id.to_string(), 38),
            view.relation_type,
            truncate(&view.other_asset_name, 30),
            view.provenance.as_str(),
            view.note
                .as_deref()
                .map(|n| format!(" — {n}"))
                .unwrap_or_default()
        );
    }
    println!("\n{} relation(s)", views.len());
}

/// Unified library detail: shared identity fields, then the owning module's
/// own fields. The module printers are the same ones `media get` /
/// `software get` / `service get` use, so a module field is never formatted
/// twice.
pub fn print_asset_detail(view: &AssetDetailView) {
    let asset = &view.asset;
    println!("ID:          {}", asset.id);
    println!("Kind:        {}", asset.kind);
    println!("Name:        {}", asset.name);
    if let Some(summary) = &asset.summary {
        println!("Summary:     {summary}");
    }
    println!("Lifecycle:   {}", asset.lifecycle_state.as_str());
    println!("Updated:     {}", asset.updated_at.format("%Y-%m-%d"));
    print_tags("Tags:        ", &view.tags);
    print_refs(&view.external_refs);
    match &view.details {
        AssetDetails::Media(record) => print_media_record_fields(record),
        AssetDetails::Software(record) => print_software_record_fields(record),
        AssetDetails::Service(record) => print_service_record_fields(record),
    }
}

/// Unified library page: one row per asset regardless of module.
pub fn print_asset_list(page: &Page<AssetSummary>, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(page).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }

    if page.items.is_empty() {
        println!("(no assets)");
        return;
    }
    println!(
        "{:38}  {:<16}  {:<9}  {:<24}  {:<20}  UPDATED",
        "ID", "KIND", "LIFECYCLE", "NAME", "SUMMARY"
    );
    for row in &page.items {
        println!(
            "{:38}  {:<16}  {:<9}  {:<24}  {:<20}  {}",
            truncate(&row.id.to_string(), 38),
            truncate(row.kind.as_str(), 16),
            row.lifecycle.as_str(),
            truncate(&row.name, 24),
            truncate(row.subtitle.as_deref().unwrap_or("-"), 20),
            row.updated_at.format("%Y-%m-%d"),
        );
    }
    match page.total {
        Some(total) => println!(
            "\n{} asset(s) on this page, {total} total",
            page.items.len()
        ),
        None => println!("\n{} asset(s) on this page", page.items.len()),
    }
}

/// One-hop neighbour rows: the other asset's unified summary plus the edge,
/// with the effective relation type already resolved for the queried asset.
pub fn print_neighbor_views(views: &[NeighborView], json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(views).unwrap_or_else(|_| "[]".to_string())
        );
        return;
    }
    if views.is_empty() {
        println!("(no neighbors)");
        return;
    }
    for view in views {
        println!(
            "{}  {}  {}  {}{}",
            truncate(&view.asset.id.to_string(), 38),
            view.edge.relation_type,
            truncate(&view.asset.name, 30),
            view.asset.kind,
            view.edge
                .note
                .as_deref()
                .map(|n| format!(" — {n}"))
                .unwrap_or_default()
        );
    }
    println!("\n{} neighbor(s)", views.len());
}

/// A traversal result: depth-ordered nodes with the path that reached them.
pub fn print_graph_nodes(view: &TraversalView, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(view).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    if view.nodes.is_empty() {
        println!("(no nodes)");
    } else {
        println!("{:<5}  {:38}  {:<24}  PATH", "DEPTH", "ASSET", "NAME");
        for node in &view.nodes {
            println!(
                "{:<5}  {:38}  {:<24}  {}",
                node.depth,
                truncate(&node.asset.id.to_string(), 38),
                truncate(&node.asset.name, 24),
                describe_path(&node.path),
            );
        }
        println!("\n{} node(s)", view.nodes.len());
    }
    if view.truncated {
        println!("(depth bound reached; deeper nodes may exist)");
    }
}

/// Renders one path as `A --type--> B --type--> C`, using each hop's own
/// effective type so no inverse is derived here.
fn describe_path(
    path: &[assetmesh_core::application::relation_query_service::RelationPathHop],
) -> String {
    if path.is_empty() {
        return "-".to_string();
    }
    let mut rendered = truncate(&path[0].from_asset_id.to_string(), 8);
    for hop in path {
        rendered.push_str(&format!(
            " --{}--> {}",
            hop.relation_type,
            truncate(&hop.to_asset_id.to_string(), 8)
        ));
    }
    rendered
}

/// Cross-module activity rows, newest first.
pub fn print_activity_page(page: &Page<ActivityView>, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(page).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    if page.items.is_empty() {
        println!("(no activity)");
        return;
    }
    for event in &page.items {
        let module = event
            .module
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "?".into());
        let asset = event.asset_name.clone().unwrap_or_else(|| "-".to_string());
        println!(
            "{}  {:<24}  {:<9}  {:<28}  {}",
            event.occurred_at.format("%Y-%m-%d %H:%M"),
            truncate(&event.event_type, 24),
            module,
            truncate(&asset, 28),
            event.actor,
        );
    }
    match page.total {
        Some(total) => println!(
            "\n{} event(s) on this page, {total} total",
            page.items.len()
        ),
        None => println!("\n{} event(s) on this page", page.items.len()),
    }
}

/// Duplicate review rows: evidence only. Merging stays an explicit command.
pub fn print_duplicate_candidates(page: &Page<DuplicateCandidate>, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(page).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    if page.items.is_empty() {
        println!("(no duplicate candidates)");
        return;
    }
    for candidate in &page.items {
        println!("{}", truncate(&candidate.left.id.to_string(), 38));
        println!(
            "  {}  {}  {}",
            truncate(&candidate.left.name, 30),
            candidate.left.kind,
            candidate.left.lifecycle.as_str()
        );
        println!("{}", truncate(&candidate.right.id.to_string(), 38));
        println!(
            "  {}  {}  {}",
            truncate(&candidate.right.name, 30),
            candidate.right.kind,
            candidate.right.lifecycle.as_str()
        );
        for evidence in &candidate.evidence {
            println!("  evidence: {}", evidence.label());
        }
        println!();
    }
    match page.total {
        Some(total) => println!(
            "{} candidate pair(s) on this page, {total} total",
            page.items.len()
        ),
        None => println!("{} candidate pair(s) on this page", page.items.len()),
    }
}
