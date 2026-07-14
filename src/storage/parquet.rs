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
    T: Serialize + DeserializeOwned,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // The schema is inferred from the rows, which is only safe because our
    // domain enums serialize as plain STRINGS rather than as serde enums (see
    // the `serde(into/try_from = "String")` attributes on SourceKind, NewsScope,
    // and SentimentLabel).
    //
    // That indirection is load-bearing, not decoration. Stage 1's real data
    // contains no `NewsScope::SectorTheme` article at all — Massive articles
    // always carry tickers, GDELT articles never do — which left a HOLE in the
    // middle of the enum's variant indices. serde_arrow traced the field as a
    // Union with a `Null`-typed `UnknownVariant` placeholder and refused to
    // write the file. The fixture survived only by luck: its one unobserved
    // variant happened to be the *last* one.
    //
    // A snapshot's schema must not depend on which values its data happens to
    // contain. `from_type` would fix that too, but it cannot see through
    // `DateTime<Utc>` (chrono is not self-describing), so we make the enums
    // self-describing instead.
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
