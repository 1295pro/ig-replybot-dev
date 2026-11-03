use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Url, redirect::Policy};
use serde_json::Value;
use tokio::sync::watch;

use crate::{
    config::Config,
    storage::{Database, PolledComment},
};

pub async fn run(
    mut config_updates: watch::Receiver<Arc<Config>>,
    database: Arc<Database>,
    last_success: Arc<AtomicU64>,
) {
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("could not start comment poller: {error}");
            return;
        }
    };
    let mut previous_counts = HashMap::new();
    let mut followup_scans = HashMap::new();
    let mut completed_sweeps = 0u64;
    let mut active_account = String::new();
    let mut first_poll = true;

    loop {
        let current = config_updates.borrow_and_update().clone();
        if !current.automation_enabled || !current.poll_enabled {
            tokio::select! {
                _ = config_updates.changed() => {},
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
            }
            continue;
        }
        if !first_poll {
            tokio::select! {
                _ = config_updates.changed() => continue,
                _ = tokio::time::sleep(Duration::from_secs(current.poll_interval_seconds)) => {},
            }
        }
        first_poll = false;
        let config = config_updates.borrow_and_update().clone();
        if !config.automation_enabled || !config.poll_enabled {
            continue;
        }
        let account = config.ig_user_id.as_deref().unwrap_or_default();
        if active_account != account {
            active_account = account.to_owned();
            previous_counts.clear();
            followup_scans.clear();
            completed_sweeps = 0;
            last_success.store(0, Ordering::Relaxed);
        }
        let full_scan = completed_sweeps.is_multiple_of(10);
        let sweep = fetch_sweep(
            &client,
            "https://graph.instagram.com",
            &config,
            &previous_counts,
            &followup_scans,
            full_scan,
        )
        .await;
        if config_updates.has_changed().unwrap_or(true) {
            continue;
        }
        match sweep {
            Ok((comments, counts, next_followups)) => {
                let account_id = config.ig_user_id.as_deref().unwrap_or_default();
                match database.ingest_poll(&comments, account_id, &config.keyword) {
                    Ok(result) => {
                        if result.baseline_created {
                            eprintln!(
                                "Instagram poll baseline created from {} existing comment(s)",
                                comments.len()
                            );
                        } else if result.new_comments > 0 {
                            eprintln!(
                                "Instagram poll found {} new comment(s); queued {} matching reply job(s)",
                                result.new_comments, result.matched_comments
                            );
                        }
                        previous_counts = counts;
                        followup_scans = next_followups;
                        completed_sweeps += 1;
                        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
                            last_success.store(now.as_secs(), Ordering::Relaxed);
                        }
                    }
                    Err(error) => eprintln!("could not save Instagram poll: {error}"),
                }
            }
            Err(error) => eprintln!("Instagram comment poll failed: {error}"),
        }
    }
}

async fn fetch_sweep(
    client: &Client,
    origin: &str,
    config: &Config,
    previous_counts: &HashMap<String, i64>,
    followup_scans: &HashMap<String, u8>,
    full_scan: bool,
) -> Result<
    (
        Vec<PolledComment>,
        HashMap<String, i64>,
        HashMap<String, u8>,
    ),
    String,
> {
    let account_id = config
        .ig_user_id
        .as_deref()
        .ok_or("IG_USER_ID is missing")?;
    let media = fetch_collection(
        client,
        origin,
        config,
        &format!("{account_id}/media"),
        "id,comments_count",
    )
    .await?;
    let mut counts = HashMap::new();
    let mut next_followups = HashMap::new();
    let mut comments = Vec::new();
    for item in media {
        let Some(media_id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !valid_id(media_id) {
            return Err("Meta returned an invalid media ID".to_owned());
        }
        let count = item
            .get("comments_count")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        counts.insert(media_id.to_owned(), count);
        let remaining = followup_scans.get(media_id).copied().unwrap_or(0);
        let (scan, next_remaining) = scan_plan(
            previous_counts.get(media_id).copied(),
            count,
            remaining,
            full_scan,
        );
        if next_remaining > 0 {
            next_followups.insert(media_id.to_owned(), next_remaining);
        }
        if !scan {
            continue;
        }
        let rows = fetch_collection(
            client,
            origin,
            config,
            &format!("{media_id}/comments"),
            "id,text,from,username,timestamp",
        )
        .await?;
        for row in rows {
            let (Some(comment_id), Some(text), Some(commenter_id)) = (
                row.get("id").and_then(Value::as_str),
                row.get("text").and_then(Value::as_str),
                row.get("from")
                    .and_then(|from| from.get("id"))
                    .and_then(Value::as_str),
            ) else {
                continue;
            };
            if !valid_id(comment_id) || !valid_id(commenter_id) {
                continue;
            }
            comments.push(PolledComment {
                comment_id: comment_id.to_owned(),
                media_id: media_id.to_owned(),
                commenter_id: commenter_id.to_owned(),
                is_owner: row.get("username").and_then(Value::as_str)
                    == Some(config.ig_username.as_str())
                    || row
                        .get("from")
                        .and_then(|from| from.get("username"))
                        .and_then(Value::as_str)
                        == Some(config.ig_username.as_str()),
                text: text.to_owned(),
            });
        }
    }
    Ok((comments, counts, next_followups))
}

fn scan_plan(previous_count: Option<i64>, count: i64, remaining: u8, full: bool) -> (bool, u8) {
    let changed = previous_count != Some(count);
    let next_remaining = if changed {
        3
    } else {
        remaining.saturating_sub(1)
    };
    (full || changed || remaining > 0, next_remaining)
}

async fn fetch_collection(
    client: &Client,
    origin: &str,
    config: &Config,
    path: &str,
    fields: &str,
) -> Result<Vec<Value>, String> {
    let version = config
        .api_version
        .strip_prefix('v')
        .unwrap_or(&config.api_version);
    let endpoint = format!("{origin}/v{version}/{path}");
    let mut rows = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..20 {
        let mut url = Url::parse(&endpoint).map_err(|_| "invalid Meta API URL".to_owned())?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("fields", fields)
                .append_pair("limit", "100");
            if let Some(cursor) = &after {
                query.append_pair("after", cursor);
            }
        }
        let response = client
            .get(url)
            .bearer_auth(config.ig_access_token.as_deref().unwrap_or_default())
            .send()
            .await
            .map_err(|error| format!("Meta request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Meta returned HTTP {} while polling",
                response.status()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Meta returned invalid poll JSON".to_owned())?;
        let page = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or("Meta poll response is missing data")?;
        rows.extend(page.iter().cloned());
        after = body
            .get("paging")
            .and_then(|paging| paging.get("cursors"))
            .and_then(|cursors| cursors.get("after"))
            .and_then(Value::as_str)
            .filter(|_| {
                body.get("paging")
                    .and_then(|paging| paging.get("next"))
                    .is_some()
            })
            .map(str::to_owned);
        if after.is_none() {
            return Ok(rows);
        }
    }
    Err("Meta poll exceeded 20 pages; increase coverage before relying on polling".to_owned())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::scan_plan;

    #[test]
    fn rechecks_a_changed_count_even_after_the_count_stops_changing() {
        assert_eq!(scan_plan(Some(4), 5, 0, false), (true, 3));
        assert_eq!(scan_plan(Some(5), 5, 3, false), (true, 2));
        assert_eq!(scan_plan(Some(5), 5, 2, false), (true, 1));
        assert_eq!(scan_plan(Some(5), 5, 1, false), (true, 0));
        assert_eq!(scan_plan(Some(5), 5, 0, false), (false, 0));
    }

    #[test]
    fn full_sweep_reads_comments_even_when_meta_reports_zero() {
        assert_eq!(scan_plan(Some(0), 0, 0, true), (true, 0));
    }
}
