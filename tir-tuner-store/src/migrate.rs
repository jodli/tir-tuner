//! refinery driver for DuckDB, modeled on refinery's own rusqlite driver.

use duckdb::Connection;
use refinery::{Migrate, Migration};
use refinery_core::traits::sync::{Query, Transaction};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) struct Duck<'a>(pub &'a mut Connection);

impl Transaction for Duck<'_> {
    type Error = duckdb::Error;

    fn execute<'a, T: Iterator<Item = &'a str>>(&mut self, queries: T) -> duckdb::Result<usize> {
        let tx = self.0.transaction()?;
        let mut count = 0;
        for q in queries {
            tx.execute_batch(q)?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }
}

impl Query<Vec<Migration>> for Duck<'_> {
    fn query(&mut self, query: &str) -> duckdb::Result<Vec<Migration>> {
        let mut stmt = self.0.prepare(query)?;
        let rows = stmt.query_map([], |r| {
            let applied_on: String = r.get(2)?;
            let checksum: String = r.get(3)?;
            Ok(Migration::applied(
                r.get(0)?,
                r.get(1)?,
                // refinery wrote both columns itself, so they parse.
                OffsetDateTime::parse(&applied_on, &Rfc3339).expect("RFC 3339 applied_on"),
                checksum.parse().expect("u64 checksum"),
            ))
        })?;
        rows.collect()
    }
}

impl Migrate for Duck<'_> {}
