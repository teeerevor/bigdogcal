use async_trait::async_trait;
use axum::Router as AxumRouter;
use loco_rs::{
    app::{AppContext, Initializer},
    environment::Environment,
    Result,
};

use crate::workers::ics_sync;

const DEFAULT_ICS_URL: &str =
    "https://user.fm/calendar/v1-c78bb1731c991cc545c9152650dad514/bigdogcal.ics";

/// Starts the recurring ICS calendar sync as a plain Tokio background task
/// once the app boots (skipped in tests, so `cargo test` doesn't hit the
/// network).
pub struct IcsSyncInitializer;

#[async_trait]
impl Initializer for IcsSyncInitializer {
    fn name(&self) -> String {
        "ics-sync".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
        if ctx.environment != Environment::Test {
            let url =
                std::env::var("ICS_CALENDAR_URL").unwrap_or_else(|_| DEFAULT_ICS_URL.to_string());
            ics_sync::spawn(ctx.clone(), url);
        }

        Ok(router)
    }
}
