use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use arrow::array::StringArray;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;

use crate::models::ParsedRuc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Csv,
    Json,
    Neon,
    Parquet,
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "neon" => Some(Self::Neon),
            "parquet" => Some(Self::Parquet),
            _ => None,
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Neon => "neon",
            Self::Parquet => "parquet",
        }
    }
}

pub fn export(records: &[ParsedRuc], format: ExportFormat, path: &Path) -> Result<(), String> {
    match format {
        ExportFormat::Csv => export_csv(records, path),
        ExportFormat::Json => export_json(records, path),
        ExportFormat::Neon => export_neon(records, path),
        ExportFormat::Parquet => export_parquet(records, path),
    }
}

fn export_csv(records: &[ParsedRuc], path: &Path) -> Result<(), String> {
    let mut wtr = csv::Writer::from_path(path).map_err(|e| e.to_string())?;
    for r in records {
        wtr.serialize(r).map_err(|e| e.to_string())?;
    }
    wtr.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn export_json(records: &[ParsedRuc], path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Exports to NEON (Neural Efficient Object Notation) strict mode.
/// Format: tabular with header `records#N^col1,col2,...` followed by space-separated rows.
fn export_neon(records: &[ParsedRuc], path: &Path) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;

    let header = format!(
        "records#{}^ruc,first_names,last_names,full_name,check_digit,old_ruc,status\n",
        records.len()
    );
    file.write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;

    for r in records {
        let line = format!(
            "  {} {} {} {} {} {} {}\n",
            neon_escape(&r.ruc),
            neon_escape(&r.first_names),
            neon_escape(&r.last_names),
            neon_escape(&r.full_name),
            neon_escape(&r.check_digit),
            neon_escape(&r.old_ruc),
            neon_escape(&r.status),
        );
        file.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// In NEON strict mode, strings with spaces are quoted. Empty strings become `""`.
fn neon_escape(s: &str) -> String {
    if s.is_empty() {
        return r#""""#.to_string();
    }
    if s.contains(' ') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn export_parquet(records: &[ParsedRuc], path: &Path) -> Result<(), String> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("ruc", DataType::Utf8, false),
        Field::new("first_names", DataType::Utf8, false),
        Field::new("last_names", DataType::Utf8, false),
        Field::new("full_name", DataType::Utf8, false),
        Field::new("check_digit", DataType::Utf8, false),
        Field::new("old_ruc", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(
                records.iter().map(|r| r.ruc.as_str()).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.first_names.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.last_names.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.full_name.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.check_digit.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.old_ruc.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                records
                    .iter()
                    .map(|r| r.status.as_str())
                    .collect::<Vec<_>>(),
            )),
        ],
    )
    .map_err(|e| e.to_string())?;

    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut writer = ArrowWriter::try_new(file, schema, None).map_err(|e| e.to_string())?;
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neon_escape_simple() {
        assert_eq!(neon_escape("ACTIVO"), "ACTIVO");
    }

    #[test]
    fn test_neon_escape_with_spaces() {
        assert_eq!(neon_escape("JUANA DEL CARMEN"), "\"JUANA DEL CARMEN\"");
    }

    #[test]
    fn test_neon_escape_empty() {
        assert_eq!(neon_escape(""), r#""""#);
    }

    #[test]
    fn test_export_format_from_str() {
        assert_eq!(ExportFormat::from_str("csv"), Some(ExportFormat::Csv));
        assert_eq!(ExportFormat::from_str("JSON"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_str("Neon"), Some(ExportFormat::Neon));
        assert_eq!(
            ExportFormat::from_str("parquet"),
            Some(ExportFormat::Parquet)
        );
        assert_eq!(ExportFormat::from_str("xml"), None);
    }
}
