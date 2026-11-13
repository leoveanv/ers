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

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let existed = path.exists();
        let mut connection = Connection::open(path)?;
        configure_connection(&connection)?;
        if existed {
            check_version(&connection)?;
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

fn check_version(connection: &Connection) -> Result<()> {
    if schema_version(connection)? != 1 {
        return Err(Error::Compatibility(
            "The ers database schema is not supported by this reader.".into(),
        ));
    }
    Ok(())
}

fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(include_str!("storage/schema.sql"))?;
    validate_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", 1)?;
    transaction.commit()?;
    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<()> {
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
