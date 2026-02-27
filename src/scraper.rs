use chrono::NaiveDate;
use log::info;
use sqlx::PgPool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::PathBuf;
use zip::ZipArchive;

use crate::config::AppConfig;
use crate::exporter::{self, ExportFormat};
use crate::models::ParsedRuc;
use crate::repository;

/// A discovered RUC ZIP file from the DNIT page.
struct RucFile {
    file_name: String,
    url: String,
    index: u32,
}

/// Sync to database.
pub async fn run_sync_db(pool: &PgPool, config: &AppConfig) {
    info!("Fetching page: {}...", config.sync_page_url);
    let html = match fetch_page(&config.sync_page_url).await {
        Ok(h) => h,
        Err(e) => {
            log::error!("Failed to fetch DNIT page: {e}");
            return;
        }
    };

    let site_date = match parse_reference_date(&html) {
        Ok(d) => {
            info!("Site reference date: {d}");
            d
        }
        Err(e) => {
            log::error!("Failed to parse reference date: {e}");
            log::error!("Aborting sync — cannot verify if data is up to date.");
            return;
        }
    };

    let last_date = match repository::get_last_reference_date(pool).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to query last reference date from DB: {e}");
            None
        }
    };

    if let Some(last) = last_date {
        if last == site_date {
            info!("Data is already up to date (reference date: {last}). Skipping sync.");
            return;
        }
        info!("New data available: DB has {last}, site has {site_date}. Starting import...");
    } else {
        info!("No previous data in DB. Starting first import...");
    }

    let files = discover_zip_urls(&html, &config.sync_page_url);
    if files.is_empty() {
        log::error!("No ruc*.zip URLs found on page. Aborting.");
        return;
    }
    info!("Discovered {} ZIP files on page.", files.len());

    for ruc_file in &files {
        info!("Downloading {}...", ruc_file.url);
        let bytes = match download(&ruc_file.url).await {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to download {}: {e}", ruc_file.file_name);
                continue;
            }
        };

        let current_hash = compute_hash(&bytes);
        let last_hash = repository::get_last_file_hash(pool, &ruc_file.file_name)
            .await
            .unwrap_or(None);

        if let Some(stored) = last_hash {
            if stored == current_hash {
                info!("{}: unchanged (hash match). Skipping.", ruc_file.file_name);
                continue;
            }
            info!(
                "{}: file changed (hash mismatch). Processing...",
                ruc_file.file_name
            );
        } else {
            info!("{}: no previous hash. Processing...", ruc_file.file_name);
        }

        info!("Inserting metadata for {}...", ruc_file.file_name);
        let file_metadata_id = match repository::insert_file_metadata(
            pool,
            &ruc_file.file_name,
            &ruc_file.url,
            Some(site_date),
            current_hash,
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                log::error!("Failed to insert metadata for {}: {e}", ruc_file.file_name);
                continue;
            }
        };

        info!("Extracting and parsing {}...", ruc_file.file_name);
        let records = match extract_and_parse(&bytes, ruc_file.index) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to extract {}: {e}", ruc_file.file_name);
                continue;
            }
        };

        info!(
            "Upserting {} records from {}...",
            records.len(),
            ruc_file.file_name
        );
        let mut inserted = 0usize;
        for chunk in records.chunks(config.sync_batch_size) {
            if let Err(e) = repository::upsert_ruc_batch(pool, chunk, file_metadata_id).await {
                log::error!("Failed to upsert batch from {}: {e}", ruc_file.file_name);
                continue;
            }
            inserted += chunk.len();
            info!(
                "  {}: {inserted}/{} records",
                ruc_file.file_name,
                records.len()
            );
        }
        info!(
            "Finished {}: {inserted} records upserted.",
            ruc_file.file_name
        );
    }
    info!("Sync complete (reference date: {site_date}).");
}

/// Sync to file (no database needed).
pub async fn run_sync_file(format: ExportFormat, output_dir: &str, config: &AppConfig) {
    info!("Fetching page: {}...", config.sync_page_url);
    let html = match fetch_page(&config.sync_page_url).await {
        Ok(h) => h,
        Err(e) => {
            log::error!("Failed to fetch DNIT page: {e}");
            return;
        }
    };

    let site_date = match parse_reference_date(&html) {
        Ok(d) => {
            info!("Site reference date: {d}");
            d
        }
        Err(e) => {
            log::error!("Failed to parse reference date: {e}");
            log::error!("Aborting sync.");
            return;
        }
    };

    let dir = PathBuf::from(output_dir);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).expect("Failed to create output directory");
    }

    let sentinel = dir.join(format!("ruc_{}.{}", site_date, format.extension()));
    if sentinel.exists() {
        info!(
            "Data is already up to date (file {} exists). Skipping sync.",
            sentinel.display()
        );
        return;
    }

    let files = discover_zip_urls(&html, &config.sync_page_url);
    if files.is_empty() {
        log::error!("No ruc*.zip URLs found on page. Aborting.");
        return;
    }
    info!("Discovered {} ZIP files on page.", files.len());

    let mut all_records: Vec<ParsedRuc> = Vec::new();

    for ruc_file in &files {
        info!("Downloading {}...", ruc_file.url);
        let bytes = match download(&ruc_file.url).await {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to download {}: {e}", ruc_file.file_name);
                continue;
            }
        };

        info!("Extracting and parsing {}...", ruc_file.file_name);
        let records = match extract_and_parse(&bytes, ruc_file.index) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to extract {}: {e}", ruc_file.file_name);
                continue;
            }
        };

        info!(
            "Parsed {} records from {}.",
            records.len(),
            ruc_file.file_name
        );
        all_records.extend(records);
    }

    let output_path = dir.join(format!("ruc_{}.{}", site_date, format.extension()));
    info!(
        "Exporting {} total records to {}...",
        all_records.len(),
        output_path.display()
    );

    match exporter::export(&all_records, format, &output_path) {
        Ok(()) => info!("Export complete: {}", output_path.display()),
        Err(e) => log::error!("Export failed: {e}"),
    }
}

/// Fetch a page's HTML body.
async fn fetch_page(url: &str) -> Result<String, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Discover all ruc*.zip download URLs from the DNIT page HTML.
/// Looks for href attributes containing "ruc" and ".zip".
fn discover_zip_urls(html: &str, page_url: &str) -> Vec<RucFile> {
    let base_url = extract_base_url(page_url);
    let mut files: Vec<RucFile> = Vec::new();

    // Find all href="...rucN.zip..." patterns
    for href_start in html.match_indices("href=\"") {
        let after = &html[href_start.0 + 6..];
        let Some(end) = after.find('"') else {
            continue;
        };
        let href = &after[..end];

        // Match URLs containing rucN.zip (where N is one or more digits)
        let Some(ruc_pos) = href.find("ruc") else {
            continue;
        };
        let after_ruc = &href[ruc_pos + 3..];
        let Some(zip_pos) = after_ruc.find(".zip") else {
            continue;
        };
        let digits = &after_ruc[..zip_pos];
        let Ok(index) = digits.parse::<u32>() else {
            continue;
        };

        let file_name = format!("ruc{index}.zip");
        let url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{base_url}{href}")
        };

        // Avoid duplicates
        if files.iter().any(|f| f.file_name == file_name) {
            continue;
        }

        files.push(RucFile {
            file_name,
            url,
            index,
        });
    }

    files.sort_by_key(|f| f.index);
    files
}

/// Extract base URL (scheme + host) from a full URL.
fn extract_base_url(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after_scheme = &url[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            return url[..pos + 3 + slash].to_string();
        }
    }
    url.to_string()
}

/// Parses the reference date from the DNIT page HTML.
/// Looks for "Actualizado al D-MM-YY" pattern.
fn parse_reference_date(html: &str) -> Result<NaiveDate, String> {
    let marker = "Actualizado al";
    let pos = html
        .find(marker)
        .ok_or_else(|| "Could not find 'Actualizado al' on page".to_string())?;

    let after = &html[pos + marker.len()..];
    let date_str: String = after
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();

    if date_str.is_empty() {
        return Err("Empty date after 'Actualizado al'".to_string());
    }

    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("Unexpected date format: '{date_str}'"));
    }

    let day: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid day: '{}'", parts[0]))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid month: '{}'", parts[1]))?;
    let year_short: i32 = parts[2]
        .parse()
        .map_err(|_| format!("Invalid year: '{}'", parts[2]))?;

    let year = 2000 + year_short;

    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| format!("Invalid date: {year}-{month:02}-{day:02}"))
}

/// Computes a stable i64 hash of the given bytes.
fn compute_hash(data: &[u8]) -> i64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish() as i64
}

/// Downloads all files from DB metadata that have no file_hash,
/// saves them to download_dir, computes hash, and updates the DB.
pub async fn backfill_file_hashes(pool: &PgPool, config: &AppConfig) {
    let rows = match repository::get_metadata_without_hash(pool).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to query metadata without hash: {e}");
            return;
        }
    };

    if rows.is_empty() {
        info!("All file metadata rows already have a file_hash. Nothing to do.");
        return;
    }

    info!("{} metadata rows without file_hash found.", rows.len());

    let tmp_dir = PathBuf::from(&config.download_dir);
    if !tmp_dir.exists() {
        std::fs::create_dir_all(&tmp_dir).expect("Failed to create download directory");
    }

    for row in &rows {
        let url = match &row.file_url {
            Some(u) if !u.is_empty() => u.clone(),
            _ => {
                log::warn!(
                    "Row id={} ({}) has no file_url. Skipping.",
                    row.id,
                    row.file_name
                );
                continue;
            }
        };

        info!("Downloading {} (id={})...", url, row.id);
        let bytes = match download(&url).await {
            Ok(b) => b,
            Err(e) => {
                log::error!(
                    "Failed to download {} for id={}: {e}",
                    row.file_name,
                    row.id
                );
                continue;
            }
        };

        let file_path = tmp_dir.join(&row.file_name);
        if let Err(e) = std::fs::write(&file_path, &bytes) {
            log::error!("Failed to write {}: {e}", file_path.display());
            continue;
        }
        info!("Saved {} ({} bytes)", file_path.display(), bytes.len());

        let hash = compute_hash(&bytes);
        match repository::update_file_hash(pool, row.id, hash).await {
            Ok(()) => info!(
                "Updated file_hash for id={} ({}): {hash}",
                row.id, row.file_name
            ),
            Err(e) => log::error!("Failed to update file_hash for id={}: {e}", row.id),
        }
    }

    info!("Backfill complete.");
}

async fn download(url: &str) -> Result<Vec<u8>, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

fn extract_and_parse(zip_bytes: &[u8], index: u32) -> Result<Vec<ParsedRuc>, String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let txt_name = format!("ruc{index}.txt");
    let mut file = archive.by_name(&txt_name).map_err(|e| e.to_string())?;

    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| e.to_string())?;

    let records: Vec<ParsedRuc> = content.lines().filter_map(parse_line).collect();
    Ok(records)
}

/// Parses a line from the RUC TXT file.
/// Parses from the extremities to handle pipes inside names (section 2.2).
/// Format: RUC|NAME|CHECK_DIGIT|OLD_RUC|STATUS|
fn parse_line(line: &str) -> Option<ParsedRuc> {
    let line = line.trim().trim_end_matches('|');
    if line.is_empty() {
        return None;
    }

    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

    let ruc = parts[0].trim().to_string();
    if ruc.is_empty() {
        return None;
    }
    let status = parts[parts.len() - 1].trim().to_string();
    let old_ruc = parts[parts.len() - 2].trim().to_string();
    let check_digit = parts[parts.len() - 3].trim().to_string();

    let raw_name = parts[1..parts.len() - 3].join("|");
    let raw_name = raw_name.trim();

    let (last_names, first_names) = match raw_name.find(',') {
        Some(pos) => (
            raw_name[..pos].trim().to_string(),
            raw_name[pos + 1..].trim().to_string(),
        ),
        None => (String::new(), raw_name.to_string()),
    };

    let full_name = if last_names.is_empty() {
        first_names.clone()
    } else if first_names.is_empty() {
        last_names.clone()
    } else {
        format!("{first_names} {last_names}")
    };

    Some(ParsedRuc {
        ruc,
        first_names,
        last_names,
        full_name,
        check_digit,
        old_ruc,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reference_date() {
        let html = r#"<span>Actualizado al 1-02-26</span>"#;
        let date = parse_reference_date(html).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
    }

    #[test]
    fn test_parse_reference_date_two_digit_day() {
        let html = r#"<p>Actualizado al 15-12-25</p>"#;
        let date = parse_reference_date(html).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2025, 12, 15).unwrap());
    }

    #[test]
    fn test_discover_zip_urls() {
        let html = r#"
            <a href="/documents/20123/2726771/ruc0.zip/uuid1?t=123">ruc0</a>
            <a href="/documents/20123/2726771/ruc1.zip/uuid2?t=456">ruc1</a>
            <a href="/documents/99999/9999999/ruc9.zip/uuid3?t=789">ruc9</a>
            <a href="/other/file.pdf">not a zip</a>
        "#;
        let files = discover_zip_urls(html, "https://www.dnit.gov.py/web/portal");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].file_name, "ruc0.zip");
        assert_eq!(files[0].index, 0);
        assert!(
            files[0]
                .url
                .starts_with("https://www.dnit.gov.py/documents/")
        );
        assert_eq!(files[1].file_name, "ruc1.zip");
        assert_eq!(files[2].file_name, "ruc9.zip");
    }

    #[test]
    fn test_discover_zip_urls_absolute() {
        let html = r#"<a href="https://cdn.example.com/ruc5.zip">ruc5</a>"#;
        let files = discover_zip_urls(html, "https://www.dnit.gov.py/page");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].url, "https://cdn.example.com/ruc5.zip");
    }

    #[test]
    fn test_discover_zip_urls_no_duplicates() {
        let html = r#"
            <a href="/docs/ruc0.zip/a?t=1">link1</a>
            <a href="/docs/ruc0.zip/b?t=2">link2</a>
        "#;
        let files = discover_zip_urls(html, "https://example.com/page");
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_extract_base_url() {
        assert_eq!(
            extract_base_url("https://www.dnit.gov.py/web/portal"),
            "https://www.dnit.gov.py"
        );
        assert_eq!(
            extract_base_url("http://localhost:3000/api/v1"),
            "http://localhost:3000"
        );
    }

    #[test]
    fn test_parse_line_valid() {
        let line = "1000000|CAÑETE GONZALEZ, JUANA DEL CARMEN|3|CAGJ761720E|ACTIVO|";
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.ruc, "1000000");
        assert_eq!(parsed.last_names, "CAÑETE GONZALEZ");
        assert_eq!(parsed.first_names, "JUANA DEL CARMEN");
        assert_eq!(parsed.full_name, "JUANA DEL CARMEN CAÑETE GONZALEZ");
        assert_eq!(parsed.check_digit, "3");
        assert_eq!(parsed.old_ruc, "CAGJ761720E");
        assert_eq!(parsed.status, "ACTIVO");
    }

    #[test]
    fn test_parse_line_empty() {
        assert!(parse_line("").is_none());
        assert!(parse_line("  ").is_none());
    }

    #[test]
    fn test_parse_line_insufficient_parts() {
        assert!(parse_line("1000|NAME").is_none());
    }

    #[test]
    fn test_parse_line_quotes_in_name() {
        let line = r#"80087123|ASOCIACION DE COOPERACION ESCOLAR DEL COLEGIO " MIGUEL ANGEL RODRIGUEZ"|5|ACEJ138550L|ACTIVO|"#;
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.ruc, "80087123");
        assert!(parsed.first_names.contains("MIGUEL ANGEL RODRIGUEZ"));
        assert_eq!(parsed.last_names, "");
        assert_eq!(parsed.check_digit, "5");
        assert_eq!(parsed.old_ruc, "ACEJ138550L");
        assert_eq!(parsed.status, "ACTIVO");
    }

    #[test]
    fn test_parse_line_pipe_in_name() {
        let line = "80057447|ASOCIACION DE COOPERACION ESCOLAR ESCUELA BASICA N| 211 CARLOS ANTONIO LOPEZ|8|ACEJ095930X|SUSPENSION TEMPORAL|";
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.ruc, "80057447");
        assert_eq!(parsed.last_names, "");
        assert_eq!(
            parsed.first_names,
            "ASOCIACION DE COOPERACION ESCOLAR ESCUELA BASICA N| 211 CARLOS ANTONIO LOPEZ"
        );
        assert_eq!(parsed.check_digit, "8");
        assert_eq!(parsed.old_ruc, "ACEJ095930X");
        assert_eq!(parsed.status, "SUSPENSION TEMPORAL");
    }

    #[test]
    fn test_parse_line_empty_old_ruc() {
        let line = "1000270|CACERES  DE SANCHEZ, LILIANA|7||CANCELADO|";
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.ruc, "1000270");
        assert_eq!(parsed.last_names, "CACERES  DE SANCHEZ");
        assert_eq!(parsed.first_names, "LILIANA");
        assert_eq!(parsed.check_digit, "7");
        assert_eq!(parsed.old_ruc, "");
        assert_eq!(parsed.status, "CANCELADO");
    }

    #[test]
    fn test_parse_line_backslash_in_old_ruc() {
        let line = r"1001630|CARDOZO ROMERO, FLORENCIO|9|CARF650480\|ACTIVO|";
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.ruc, "1001630");
        assert_eq!(parsed.last_names, "CARDOZO ROMERO");
        assert_eq!(parsed.first_names, "FLORENCIO");
        assert_eq!(parsed.check_digit, "9");
        assert_eq!(parsed.old_ruc, r"CARF650480\");
        assert_eq!(parsed.status, "ACTIVO");
    }

    #[test]
    fn test_parse_line_double_spaces() {
        let line = "1000200|MONTIEL  ORTIZ, CANCIO|6|VIAM651928S|CANCELADO|";
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.last_names, "MONTIEL  ORTIZ");
        assert_eq!(parsed.first_names, "CANCIO");
    }
}
