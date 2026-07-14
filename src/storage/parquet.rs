use anyhow::{Context, Result};
use arrow::datatypes::FieldRef;
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    file::properties::WriterProperties,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_arrow::schema::{SchemaLike, TracingOptions};
use std::{
    fs::{self, File},
    path::Path,
};

pub fn write_parquet<T>(path: &Path, rows: &[T]) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Trace the Arrow schema from the actual rows (Serialize) rather than the
    // type (from_type would additionally require Deserialize, which write-only
    // row types such as SourceCatalogRow do not implement).
    // `enums_without_data_as_strings` encodes our unit enums (SourceKind,
    // NewsScope, SentimentLabel) as Arrow strings, matching their snake_case
    // serde representation, instead of erroring on data-less enum variants.
    let options = TracingOptions::default().enums_without_data_as_strings(true);
    let fields = Vec::<FieldRef>::from_samples(rows, options)?;
    let batch = serde_arrow::to_record_batch(&fields, &rows)?;
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

pub fn read_parquet<T>(path: &Path) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let mut batch_rows: Vec<T> = serde_arrow::from_record_batch(&batch)?;
        rows.append(&mut batch_rows);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Row {
        id: String,
        value: f64,
    }

    #[test]
    fn parquet_round_trips_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("rows.parquet");
        let rows = vec![
            Row {
                id: "a".into(),
                value: 1.25,
            },
            Row {
                id: "b".into(),
                value: -0.75,
            },
        ];

        write_parquet(&path, &rows).unwrap();
        let loaded: Vec<Row> = read_parquet(&path).unwrap();

        assert_eq!(loaded, rows);
    }
}
