//! Human-readable output formatting for the CLI. No business logic here.

use assetmesh_core::application::import_media::ImportReport;
use assetmesh_core::application::media_service::MediaView;
use assetmesh_core::domain::media::Progress;
use assetmesh_core::domain::search::SearchHit;
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

fn fmt_time(ts: Option<chrono::DateTime<chrono::Utc>>) -> String {
    ts.map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".into())
}

pub fn print_media_detail(view: &MediaView) {
    let entry = &view.entry;
    println!("ID:       {}", entry.asset.id);
    println!("Kind:     {}", entry.asset.kind);
    println!("Title:    {}", entry.asset.name);
    if let Some(summary) = &entry.asset.summary {
        println!("Summary:  {summary}");
    }
    println!("Type:     {}", entry.record.media_type);
    println!("Status:   {}", entry.record.status);
    println!("Rating:   {}", fmt_rating(entry.record.rating));
    println!(
        "Year:     {}",
        entry
            .record
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "Platform: {}",
        entry.record.platform.as_deref().unwrap_or("-")
    );
    println!("Progress: {}", fmt_progress(&entry.record.progress));
    println!("Started:  {}", fmt_time(entry.record.started_at));
    println!("Completed: {}", fmt_time(entry.record.completed_at));
    if let Some(notes) = &entry.record.notes {
        println!("Notes:    {notes}");
    }
    if !view.tags.is_empty() {
        println!("Tags:     {}", view.tags.join(", "));
    }
    if !view.external_refs.is_empty() {
        println!("Refs:");
        for reference in &view.external_refs {
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
    if !view.activity.is_empty() {
        println!("Activity:");
        for event in &view.activity {
            println!(
                "  {}  {} [{}]  {}",
                event.occurred_at.format("%Y-%m-%d %H:%M"),
                event.event_type,
                event.actor,
                event.payload
            );
        }
    }
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
use assetmesh_core::application::software_discovery::CandidateDisposition;
use assetmesh_core::application::software_service::{AdoptionOutcome, ScanReport, SoftwareView};
use assetmesh_core::ports::repos::SoftwareListRow;

pub fn print_software_detail(view: &SoftwareView) {
    let entry = &view.entry;
    println!("ID:             {}", entry.asset.id);
    println!("Kind:           {}", entry.asset.kind);
    println!("Name:           {}", entry.asset.name);
    if let Some(summary) = &entry.asset.summary {
        println!("Summary:        {summary}");
    }
    println!("Category:       {}", entry.record.category);
    println!("Install source: {}", entry.record.install_source);
    println!(
        "Version:        {}",
        entry.record.version.as_deref().unwrap_or("-")
    );
    println!(
        "Location:       {}",
        entry.record.install_location.as_deref().unwrap_or("-")
    );
    println!(
        "Executable:     {}",
        entry.record.executable_path.as_deref().unwrap_or("-")
    );
    println!(
        "Architecture:   {}",
        entry.record.architecture.as_deref().unwrap_or("-")
    );
    println!("Discovered:     {}", fmt_time(entry.record.discovered_at));
    println!("Installed:      {}", fmt_time(entry.record.installed_at));
    if let Some(purpose) = &entry.record.purpose {
        println!("Purpose:        {purpose}");
    }
    if let Some(notes) = &entry.record.notes {
        println!("Notes:          {notes}");
    }
    if !view.tags.is_empty() {
        println!("Tags:           {}", view.tags.join(", "));
    }
    if !view.external_refs.is_empty() {
        println!("Refs:");
        for reference in &view.external_refs {
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
    if !view.activity.is_empty() {
        println!("Activity:");
        for event in &view.activity {
            println!(
                "  {}  {} [{}]  {}",
                event.occurred_at.format("%Y-%m-%d %H:%M"),
                event.event_type,
                event.actor,
                event.payload
            );
        }
    }
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
