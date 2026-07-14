use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut rows = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        rows.push(serde_json::from_str(&line)?);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        id: String,
        value: i32,
    }

    #[test]
    fn jsonl_round_trips_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("rows.jsonl");
        let rows = vec![
            Row {
                id: "a".into(),
                value: 1,
            },
            Row {
                id: "b".into(),
                value: 2,
            },
        ];

        write_jsonl(&path, &rows).unwrap();
        let loaded: Vec<Row> = read_jsonl(&path).unwrap();

        assert_eq!(loaded, rows);
    }
}
