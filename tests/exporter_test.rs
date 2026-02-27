use std::path::PathBuf;

use ruc_finder::exporter::{self, ExportFormat};
use ruc_finder::models::ParsedRuc;

fn sample_records() -> Vec<ParsedRuc> {
    vec![
        ParsedRuc {
            ruc: "1000000".to_string(),
            first_names: "JUANA DEL CARMEN".to_string(),
            last_names: "CAÑETE GONZALEZ".to_string(),
            full_name: "JUANA DEL CARMEN CAÑETE GONZALEZ".to_string(),
            check_digit: "3".to_string(),
            old_ruc: "CAGJ761720E".to_string(),
            status: "ACTIVO".to_string(),
        },
        ParsedRuc {
            ruc: "2000000".to_string(),
            first_names: "CARLOS".to_string(),
            last_names: "LOPEZ".to_string(),
            full_name: "CARLOS LOPEZ".to_string(),
            check_digit: "7".to_string(),
            old_ruc: "".to_string(),
            status: "CANCELADO".to_string(),
        },
    ]
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("ruc_finder_exporter_test")
        .join(name)
}

fn setup_temp_dir() {
    let dir = std::env::temp_dir().join("ruc_finder_exporter_test");
    std::fs::create_dir_all(&dir).unwrap();
}

// ---------------------------------------------------------------------------
// ExportFormat::from_str
// ---------------------------------------------------------------------------

#[test]
fn export_format_from_str_case_insensitive() {
    assert_eq!(ExportFormat::from_str("CSV"), Some(ExportFormat::Csv));
    assert_eq!(ExportFormat::from_str("Json"), Some(ExportFormat::Json));
    assert_eq!(ExportFormat::from_str("NEON"), Some(ExportFormat::Neon));
    assert_eq!(ExportFormat::from_str("PARQUET"), Some(ExportFormat::Parquet));
}

#[test]
fn export_format_from_str_unknown() {
    assert_eq!(ExportFormat::from_str("yaml"), None);
    assert_eq!(ExportFormat::from_str(""), None);
    assert_eq!(ExportFormat::from_str("xml"), None);
}

// ---------------------------------------------------------------------------
// ExportFormat::extension
// ---------------------------------------------------------------------------

#[test]
fn export_format_extension() {
    assert_eq!(ExportFormat::Csv.extension(), "csv");
    assert_eq!(ExportFormat::Json.extension(), "json");
    assert_eq!(ExportFormat::Neon.extension(), "neon");
    assert_eq!(ExportFormat::Parquet.extension(), "parquet");
}

// ---------------------------------------------------------------------------
// CSV export
// ---------------------------------------------------------------------------

#[test]
fn export_csv_creates_valid_file() {
    setup_temp_dir();
    let path = temp_path("test_export.csv");
    let records = sample_records();

    exporter::export(&records, ExportFormat::Csv, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    // Header row
    assert!(content.starts_with("ruc,first_names,last_names,full_name,check_digit,old_ruc,status"));
    // Data rows
    assert!(content.contains("1000000"));
    assert!(content.contains("JUANA DEL CARMEN"));
    assert!(content.contains("2000000"));
    assert!(content.contains("CANCELADO"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_csv_empty_records() {
    setup_temp_dir();
    let path = temp_path("test_empty.csv");

    exporter::export(&[], ExportFormat::Csv, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    // csv crate writes no header when there are zero records (nothing serialized)
    assert!(content.trim().is_empty());

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// JSON export
// ---------------------------------------------------------------------------

#[test]
fn export_json_creates_valid_file() {
    setup_temp_dir();
    let path = temp_path("test_export.json");
    let records = sample_records();

    exporter::export(&records, ExportFormat::Json, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["ruc"], "1000000");
    assert_eq!(parsed[0]["status"], "ACTIVO");
    assert_eq!(parsed[1]["ruc"], "2000000");
    assert_eq!(parsed[1]["old_ruc"], "");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_json_preserves_unicode() {
    setup_temp_dir();
    let path = temp_path("test_unicode.json");
    let records = sample_records();

    exporter::export(&records, ExportFormat::Json, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("CAÑETE"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_json_empty_records() {
    setup_temp_dir();
    let path = temp_path("test_empty.json");

    exporter::export(&[], ExportFormat::Json, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
    assert!(parsed.is_empty());

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// NEON export
// ---------------------------------------------------------------------------

#[test]
fn export_neon_creates_valid_file() {
    setup_temp_dir();
    let path = temp_path("test_export.neon");
    let records = sample_records();

    exporter::export(&records, ExportFormat::Neon, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    // Header with record count and column names
    assert!(lines[0].starts_with("records#2^"));
    assert!(lines[0].contains("ruc,first_names,last_names"));

    // Data rows are indented
    assert!(lines[1].starts_with("  "));
    assert!(lines[2].starts_with("  "));

    // Names with spaces are quoted
    assert!(content.contains("\"JUANA DEL CARMEN\""));
    assert!(content.contains("\"CAÑETE GONZALEZ\""));

    // Empty strings become ""
    assert!(content.contains("\"\""));

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Parquet export
// ---------------------------------------------------------------------------

#[test]
fn export_parquet_creates_file() {
    setup_temp_dir();
    let path = temp_path("test_export.parquet");
    let records = sample_records();

    exporter::export(&records, ExportFormat::Parquet, &path).unwrap();

    // File should exist and have nonzero size (parquet magic bytes)
    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 0);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_parquet_empty_records() {
    setup_temp_dir();
    let path = temp_path("test_empty.parquet");

    exporter::export(&[], ExportFormat::Parquet, &path).unwrap();

    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 0); // Parquet has metadata even with no rows

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Export to nonexistent directory should fail
// ---------------------------------------------------------------------------

#[test]
fn export_csv_fails_on_bad_path() {
    let path = PathBuf::from("/nonexistent_dir_abc123/test.csv");
    let result = exporter::export(&sample_records(), ExportFormat::Csv, &path);
    assert!(result.is_err());
}

#[test]
fn export_json_fails_on_bad_path() {
    let path = PathBuf::from("/nonexistent_dir_abc123/test.json");
    let result = exporter::export(&sample_records(), ExportFormat::Json, &path);
    assert!(result.is_err());
}
