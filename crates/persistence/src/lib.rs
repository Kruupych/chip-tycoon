#![deny(warnings)]

//! Persistence layer: DB migrations, snapshots, and telemetry export.

use anyhow::{anyhow, Result};
use parquet::basic::{Repetition, Type as PhysicalType};
use parquet::column::writer::ColumnWriter;
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::types::Type;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use sim_core as core;
use sqlx::{migrate::Migrator, Pool, Row, Sqlite, SqlitePool};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Returns the default SQLite URL used for local saves.
pub fn default_sqlite_url() -> &'static str {
    "sqlite://./saves/main.db"
}

static MIGRATIONS: Migrator = sqlx::migrate!("../../migrations/sqlite");

/// Initialize the SQLite database: connect and run migrations.
pub async fn init_db(url: &str) -> Result<SqlitePool> {
    let pool = SqlitePool::connect(url).await?;
    MIGRATIONS.run(&pool).await?;
    Ok(pool)
}

/// Insert a new save and return its id.
pub async fn create_save(
    pool: &Pool<Sqlite>,
    name: &str,
    description: Option<&str>,
) -> Result<i64> {
    let rec = sqlx::query(r#"INSERT INTO saves (name, description) VALUES (?1, ?2) RETURNING id"#)
        .bind(name)
        .bind(description)
        .fetch_one(pool)
        .await?;
    let id: i64 = rec.try_get("id").unwrap_or(0);
    Ok(id)
}

/// Serialize a world state using bincode.
pub fn serialize_world_bincode(world: &core::World) -> Result<Vec<u8>> {
    Ok(bincode::serialize(world)?)
}

/// Deserialize a world state from bincode bytes.
pub fn deserialize_world_bincode(bytes: &[u8]) -> Result<core::World> {
    Ok(bincode::deserialize(bytes)?)
}

/// Store a snapshot blob for a given save.
pub async fn insert_snapshot(
    pool: &Pool<Sqlite>,
    save_id: i64,
    month_index: i64,
    format: &str,
    data: &[u8],
) -> Result<i64> {
    let rec = sqlx::query(
        r#"INSERT INTO snapshots (save_id, month_index, format, data)
           VALUES (?1, ?2, ?3, ?4) RETURNING id"#,
    )
    .bind(save_id)
    .bind(month_index)
    .bind(format)
    .bind(data)
    .fetch_one(pool)
    .await?;
    let id: i64 = rec.try_get("id").unwrap_or(0);
    Ok(id)
}

/// Load the latest snapshot for a save.
pub async fn latest_snapshot(
    pool: &Pool<Sqlite>,
    save_id: i64,
) -> Result<Option<(i64, i64, Vec<u8>, String)>> {
    let rec = sqlx::query(
        r#"SELECT id, month_index, data, format FROM snapshots
           WHERE save_id = ?1 ORDER BY month_index DESC, id DESC LIMIT 1"#,
    )
    .bind(save_id)
    .fetch_optional(pool)
    .await?;
    Ok(rec.map(|r| {
        let id: i64 = r.try_get("id").unwrap_or(0);
        let month_index: i64 = r.try_get("month_index").unwrap_or(0);
        let data: Vec<u8> = r.try_get("data").unwrap_or_default();
        let format: String = r.try_get("format").unwrap_or_default();
        (id, month_index, data, format)
    }))
}

/// Row format for telemetry exports.
#[derive(Clone, Debug)]
pub struct TelemetryRow {
    pub month_index: u32,
    pub output_units: u64,
    pub sold_units: u64,
    pub asp_cents: i64,
    pub unit_cost_cents: i64,
    pub margin_cents: i64,
    pub revenue_cents: i64,
}

/// Convert a Decimal USD value to cents (i64), rounding to 2 decimals.
pub fn decimal_to_cents_i64(d: Decimal) -> Result<i64> {
    let scaled = d.round_dp(2) * Decimal::from(100u64);
    let val = scaled
        .to_i128()
        .ok_or_else(|| anyhow!("non-finite decimal"))?;
    if val < i64::MIN as i128 || val > i64::MAX as i128 {
        return Err(anyhow!("overflow while converting to cents"));
    }
    Ok(val as i64)
}

/// Convert cents (i64) to Decimal USD value.
pub fn cents_i64_to_decimal(cents: i64) -> Decimal {
    Decimal::from_i64(cents).unwrap() / Decimal::from(100u64)
}

/// Write telemetry rows to a Parquet file at the given path.
pub fn write_telemetry_parquet<P: AsRef<Path>>(path: P, rows: &[TelemetryRow]) -> Result<()> {
    let fields = vec![
        Type::primitive_type_builder("month_index", PhysicalType::INT32)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("output_units", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("sold_units", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("asp_cents", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("unit_cost_cents", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("margin_cents", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
        Type::primitive_type_builder("revenue_cents", PhysicalType::INT64)
            .with_repetition(Repetition::REQUIRED)
            .build()?,
    ];
    let fields_ptrs: Vec<Arc<Type>> = fields.into_iter().map(Arc::new).collect();
    let schema = Type::group_type_builder("telemetry")
        .with_fields(fields_ptrs)
        .build()?;

    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = SerializedFileWriter::new(
        file,
        std::sync::Arc::new(schema),
        std::sync::Arc::new(props),
    )?;

    let mut row_group = writer.next_row_group()?;

    // Prepare column vectors
    let col0: Vec<i32> = rows.iter().map(|r| r.month_index as i32).collect();
    let col1: Vec<i64> = rows.iter().map(|r| r.output_units as i64).collect();
    let col2: Vec<i64> = rows.iter().map(|r| r.sold_units as i64).collect();
    let col3: Vec<i64> = rows.iter().map(|r| r.asp_cents).collect();
    let col4: Vec<i64> = rows.iter().map(|r| r.unit_cost_cents).collect();
    let col5: Vec<i64> = rows.iter().map(|r| r.margin_cents).collect();
    let col6: Vec<i64> = rows.iter().map(|r| r.revenue_cents).collect();

    // Column 0
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int32ColumnWriter(w) => {
                let _ = w.write_batch(&col0, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for month_index")),
        }
        col.close()?;
    }
    // Column 1
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col1, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for output_units")),
        }
        col.close()?;
    }
    // Column 2
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col2, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for sold_units")),
        }
        col.close()?;
    }
    // Column 3
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col3, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for asp_cents")),
        }
        col.close()?;
    }
    // Column 4
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col4, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for unit_cost_cents")),
        }
        col.close()?;
    }
    // Column 5
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col5, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for margin_cents")),
        }
        col.close()?;
    }
    // Column 6
    {
        let mut col = row_group
            .next_column()?
            .ok_or_else(|| anyhow!("no column"))?;
        match col.untyped() {
            ColumnWriter::Int64ColumnWriter(w) => {
                let _ = w.write_batch(&col6, None, None)?;
            }
            _ => return Err(anyhow!("unexpected column type for revenue_cents")),
        }
        col.close()?;
    }
    row_group.close()?;
    writer.close()?;
    info!("parquet written");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn url_is_sqlite() {
        assert!(default_sqlite_url().starts_with("sqlite://"));
    }

    #[test]
    fn snapshot_roundtrip_in_memory() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let pool = init_db("sqlite::memory:").await.unwrap();
            let save_id = create_save(&pool, "test", Some("desc")).await.unwrap();
            let world = core::World {
                macro_state: core::MacroState {
                    date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                    inflation_annual: 0.02,
                    interest_rate: 0.05,
                    fx_usd_index: 100.0,
                },
                tech_tree: vec![],
                companies: vec![],
                segments: vec![],
            };
            let bytes = serialize_world_bincode(&world).unwrap();
            let _snap_id = insert_snapshot(&pool, save_id, 12, "bincode", &bytes)
                .await
                .unwrap();
            let latest = latest_snapshot(&pool, save_id).await.unwrap().unwrap();
            assert_eq!(latest.1, 12);
            let world2 = deserialize_world_bincode(&latest.2).unwrap();
            // Quick invariant: date preserved
            assert_eq!(world2.macro_state.date, world.macro_state.date);
        });
    }

    #[test]
    fn init_db_on_disk() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let base = std::path::Path::new("target/tmp_db_init");
            std::fs::create_dir_all(base).unwrap();
            let path = base.join("test.db");
            let url = format!("sqlite://{}", path.display());
            // touch the file to avoid SQLite open error on some platforms
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .append(true)
                .open(&path)
                .unwrap();
            let pool = init_db(&url).await.unwrap();
            // ensure `saves` table exists
            let name: Option<(String,)> = sqlx::query_as(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='saves'",
            )
            .fetch_optional(&pool)
            .await
            .unwrap();
            assert!(name.is_some());
        });
    }
}
