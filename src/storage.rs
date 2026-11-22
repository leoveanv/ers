mod annotation_records;
mod bookmarks;
mod export_links;
mod reading;

pub use bookmarks::bookmark_id;

use crate::{Error, Result};
use rusqlite::{Connection, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

use crate::annotations::{Annotation, ExportLink};
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSnapshot {
    pub link: ExportLink,
    pub annotations: Vec<Annotation>,
}

pub struct Store {
    connection: Connection,
}

pub const SCHEMA_VERSION: i64 = 2;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let existed = path.exists();
        let mut connection = Connection::open(path)?;
        configure_connection(&connection)?;
        if existed {
            ensure_supported_version(&mut connection)?;
        } else {
            initialize_schema(&mut connection)?;
        }
        validate_schema(&connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut connection = Connection::open_in_memory()?;
        configure_connection(&connection)?;
        initialize_schema(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i64> {
        schema_version(&self.connection)
    }
}

fn configure_connection(connection: &Connection) -> Result<()> {
    connection.busy_timeout(Duration::from_secs(10))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

fn schema_version(connection: &Connection) -> Result<i64> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn ensure_supported_version(connection: &mut Connection) -> Result<()> {
    match schema_version(connection)? {
        SCHEMA_VERSION => Ok(()),
        1 => migrate_v1_to_v2(connection),
        _ => Err(Error::Compatibility(
            "The ers database schema is not supported by this reader.".into(),
        )),
    }
}

fn migrate_v1_to_v2(connection: &mut Connection) -> Result<()> {
    validate_v1_schema(connection)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "ALTER TABLE library ADD COLUMN category TEXT;
         ALTER TABLE library ADD COLUMN tags TEXT NOT NULL DEFAULT '';",
    )?;
    validate_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(include_str!("storage/schema.sql"))?;
    validate_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn validate_v1_schema(connection: &Connection) -> Result<()> {
    for query in [
        "SELECT filepath, content_id, textwidth, offset, screen_row FROM reading_states LIMIT 0",
        "SELECT last_read, filepath, title, author, reading_progress FROM library LIMIT 0",
        "SELECT id, filepath, name, content_id, textwidth, offset, screen_row FROM bookmarks LIMIT 0",
        "SELECT id, fingerprint, title, author FROM annotation_books LIMIT 0",
        "SELECT path, book_id FROM annotation_locations LIMIT 0",
        "SELECT id, book_id, content_order, start, payload FROM annotations LIMIT 0",
        "SELECT book_id, path, digest, revision, exported_revision FROM annotation_exports LIMIT 0",
    ] {
        connection.prepare(query)?;
    }
    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<()> {
    for query in [
        "SELECT filepath, content_id, textwidth, offset, screen_row FROM reading_states LIMIT 0",
        "SELECT last_read, filepath, title, author, reading_progress, category, tags FROM library LIMIT 0",
        "SELECT id, filepath, name, content_id, textwidth, offset, screen_row FROM bookmarks LIMIT 0",
        "SELECT id, fingerprint, title, author FROM annotation_books LIMIT 0",
        "SELECT path, book_id FROM annotation_locations LIMIT 0",
        "SELECT id, book_id, content_order, start, payload FROM annotations LIMIT 0",
        "SELECT book_id, path, digest, revision, exported_revision FROM annotation_exports LIMIT 0",
    ] {
        connection.prepare(query)?;
    }
    Ok(())
}
