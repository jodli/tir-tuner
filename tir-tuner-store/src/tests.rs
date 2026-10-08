use chrono::NaiveDateTime;
use refinery::{Migration, Runner, Target};

use super::*;

fn at(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn cgm(s: &str, mg_dl: f64) -> GlucoseReading {
    GlucoseReading { at: at(s), mg_dl }
}

fn basal(s: &str, minutes: u32, rate_u_h: f64) -> BasalSegment {
    BasalSegment {
        at: at(s),
        minutes,
        rate_u_h,
    }
}

fn export(cgm: Vec<GlucoseReading>, basal: Vec<BasalSegment>) -> Export {
    Export {
        start: day("2026-09-28"),
        end: day("2026-09-29"),
        cgm,
        bg: vec![],
        boluses: vec![Bolus {
            at: at("2026-09-28 12:53"),
            carbs_g: 35.0,
            units: 2.7,
        }],
        basal,
    }
}

#[test]
fn overlapping_exports_only_add_new_rows() {
    let a = export(
        vec![
            cgm("2026-09-28 15:30", 220.0),
            cgm("2026-09-28 15:32", 211.0),
        ],
        vec![
            basal("2026-09-28 14:54", 10, 2.78),
            basal("2026-09-28 14:54", 1, 2.7),
        ],
    );
    // Like the real 10-08 export: a backfilled minute inside a's range, plus later data.
    let b = export(
        vec![
            cgm("2026-09-28 15:30", 220.0),
            cgm("2026-09-28 15:31", 215.0),
            cgm("2026-09-28 15:32", 211.0),
            cgm("2026-09-29 08:00", 120.0),
        ],
        vec![
            basal("2026-09-28 14:54", 1, 2.7),
            basal("2026-09-28 14:54", 10, 2.78),
        ],
    );
    let mut store = Store::open_in_memory().unwrap();
    let first = store.insert(&a).unwrap();
    assert_eq!(
        first,
        Inserted {
            cgm: 2,
            bg: 0,
            boluses: 1,
            basal: 2
        }
    );
    assert_eq!(
        store.insert(&b).unwrap(),
        Inserted {
            cgm: 2,
            ..Default::default()
        }
    );
    assert_eq!(store.insert(&a).unwrap(), Inserted::default());
    assert_eq!(
        store.range(day("2026-09-28"), day("2026-09-29")).unwrap(),
        b
    );
}

#[test]
fn upgrade_from_v1_keeps_data() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrations::runner()
        .set_target(Target::Version(1))
        .run(&mut Duck(&mut conn))
        .unwrap();
    conn.execute_batch(include_str!("../tests/fixtures/v1_data.sql"))
        .unwrap();

    let store = Store::open_with(conn, None, migrations::runner()).unwrap();
    let got = store.range(day("2026-09-28"), day("2026-09-28")).unwrap();
    assert_eq!(
        got.cgm,
        vec![
            cgm("2026-09-28 15:30", 220.0),
            cgm("2026-09-28 15:31", 215.5)
        ]
    );
    assert_eq!(got.bg, vec![cgm("2026-09-28 21:13", 51.0)]);
    assert_eq!(
        got.boluses,
        vec![
            Bolus {
                at: at("2026-09-28 12:53"),
                carbs_g: 35.0,
                units: 2.7
            },
            Bolus {
                at: at("2026-09-28 13:13"),
                carbs_g: 85.0,
                units: 6.5
            },
        ]
    );
    assert_eq!(
        got.basal,
        vec![
            basal("2026-09-28 14:54", 1, 2.7),
            basal("2026-09-28 14:54", 10, 2.78)
        ]
    );
}

/// Applied migrations are frozen: editing one would leave existing databases
/// on a schema the code no longer describes.
#[test]
fn migrations_match_lock_file() {
    let lock = include_str!("../migrations.lock");
    for m in migrations::runner().get_migrations() {
        let line = format!("{} {}", m.version(), m.checksum());
        assert!(
            lock.lines().any(|l| l == line),
            "V{} changed or is missing from migrations.lock. Never edit an applied \
             migration; for a new one append `{line}`.",
            m.version(),
        );
    }
}

#[test]
fn failed_migration_leaves_db_and_backup() {
    let dir = std::env::temp_dir().join(format!("tir-tuner-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.duckdb");
    let a = export(vec![cgm("2026-09-28 15:30", 220.0)], vec![]);
    Store::open(&path).unwrap().insert(&a).unwrap();

    let mut migrations = migrations::runner().get_migrations().clone();
    migrations.push(
        Migration::unapplied(
            "V999__broken",
            "ALTER TABLE cgm ADD COLUMN x INT; SELECT nope;",
        )
        .unwrap(),
    );
    let broken = Store::open_with(
        Connection::open(&path).unwrap(),
        Some(&path),
        Runner::new(&migrations),
    );
    assert!(broken.is_err());
    assert!(backup_path(&path, 1).exists());

    let store = Store::open(&path).unwrap();
    assert_eq!(schema_version(&store.conn).unwrap(), 1);
    let altered: bool = store
        .conn
        .query_row(
            "SELECT count(*) > 0 FROM information_schema.columns WHERE table_name = 'cgm' AND column_name = 'x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!altered, "the broken migration's ALTER was not rolled back");
    assert_eq!(store.range(a.start, a.end).unwrap().cgm, a.cgm);
    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
}
