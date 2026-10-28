use crate::domain::AppError;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub mod ledger;
const SCHEMA_VERSION: i64 = 2;
const INITIAL_SCHEMA: &str = include_str!("../../../migrations/0001_initial.sql");

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
}

pub struct DatabaseStatus {
    pub schema_version: i64,
    pub book_name: String,
    pub currency: String,
}

impl Database {
    pub fn open(directory: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join("ledger.sqlite3");
        let mut connection = Connection::open(&path)?;
        // Refuse newer databases before changing persistent database settings.
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(AppError::SchemaTooNew);
        }
        if version > 0 && version < SCHEMA_VERSION {
            let backups = directory.join("backups");
            std::fs::create_dir_all(&backups)?;
            let mut destination = Connection::open(backups.join(format!(
                "before-schema-{version}-{}.sqlite3",
                uuid::Uuid::new_v4()
            )))?;
            let backup = rusqlite::backup::Backup::new(&connection, &mut destination)?;
            backup.run_to_completion(100, Duration::from_millis(10), None)?;
        }
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(INITIAL_SCHEMA)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
        }
        if version < 2 {
            let transaction = connection.transaction()?;
            transaction
                .execute_batch(include_str!("../../../migrations/0002_manual_ledger.sql"))?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn status(&self) -> Result<DatabaseStatus, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let schema_version =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let (book_name, currency) = connection.query_row(
            "SELECT name, currency FROM books WHERE id = 'default'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(DatabaseStatus {
            schema_version,
            book_name,
            currency,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_reopens_without_duplicate_books() {
        let directory = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        {
            let database = Database::open(&directory).unwrap();
            assert_eq!(database.status().unwrap().schema_version, 2);
            let connection = database.connection.lock().unwrap();
            let foreign_keys: i64 = connection
                .pragma_query_value(None, "foreign_keys", |r| r.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 1);
        }
        {
            let database = Database::open(&directory).unwrap();
            assert_eq!(database.status().unwrap().currency, "CNY");
            let connection = database.connection.lock().unwrap();
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM books", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn refuses_newer_schema() {
        let directory = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        {
            let connection = Connection::open(directory.join("ledger.sqlite3")).unwrap();
            connection.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(matches!(
            Database::open(&directory),
            Err(AppError::SchemaTooNew)
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
