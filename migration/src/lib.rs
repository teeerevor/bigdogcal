#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20260723_131514_events;
mod m20260725_120000_add_uid_to_events;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260723_131514_events::Migration),
            Box::new(m20260725_120000_add_uid_to_events::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}