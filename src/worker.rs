use std::{sync::Arc, time::Duration};

use tokio::sync::watch;

use crate::{
    config::Config,
    meta_api::MetaApi,
    storage::{Database, DeliveryKind},
};

pub async fn run(mut config_updates: watch::Receiver<Arc<Config>>, database: Arc<Database>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let config = config_updates.borrow_and_update().clone();
        if !config.automation_enabled {
            continue;
        }
        let api = match MetaApi::new(&config) {
            Ok(api) => api,
            Err(error) => {
                eprintln!("Instagram sender is not ready: {error}");
                continue;
            }
        };
        for _ in 0..8 {
            if config_updates.has_changed().unwrap_or(true)
                || !config_updates.borrow().automation_enabled
            {
                break;
            }
            let action = match database.claim_next_for(config.ig_user_id.as_deref()) {
                Ok(Some(action)) => action,
                Ok(None) => break,
                Err(error) => {
                    eprintln!("delivery queue error: {error}");
                    break;
                }
            };
            let kind = match action.kind {
                DeliveryKind::PublicReply => "public reply",
                DeliveryKind::PrivateReply => "private reply",
            };
            match api.send(&action).await {
                Ok(response) if response.status.is_success() => {
                    if let Err(error) = database.finish_delivery(&action, "sent") {
                        eprintln!("could not save successful {kind} status: {error}");
                    } else {
                        eprintln!("{kind} sent");
                    }
                }
                Ok(response) => {
                    if let Err(error) = database.finish_delivery(&action, "failed") {
                        eprintln!("could not save failed {kind} status: {error}");
                    }
                    let details = response
                        .error
                        .as_ref()
                        .map(|error| format!("; {}", error.log_summary()))
                        .unwrap_or_default();
                    eprintln!(
                        "Meta rejected {kind} with HTTP {}{details}",
                        response.status.as_u16()
                    );
                }
                Err(_) => {
                    if let Err(error) = database.finish_delivery(&action, "uncertain") {
                        eprintln!("could not save uncertain {kind} status: {error}");
                    }
                    eprintln!("{kind} outcome is uncertain; it will not be retried automatically");
                }
            }
        }
    }
}
