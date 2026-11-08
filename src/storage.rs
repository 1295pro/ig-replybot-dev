use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{config::Config, domain::keyword::is_exact_keyword, webhook::MatchedComment};

pub struct PolledComment {
    pub comment_id: String,
    pub media_id: String,
    pub commenter_id: String,
    pub is_owner: bool,
    pub text: String,
}

pub struct PollResult {
    pub baseline_created: bool,
    pub new_comments: usize,
    pub matched_comments: usize,
}

#[derive(Serialize, Deserialize)]
pub struct QueueSummary {
    pub awaiting_public_reply: u64,
    pub awaiting_private_reply: u64,
    pub completed: u64,
    pub needs_attention: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentTarget {
    pub host: String,
    pub ssh_port: u16,
    pub service_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_fingerprint: Option<String>,
}

#[derive(Serialize)]
pub struct RecentActivity {
    pub received_at: i64,
    pub public_reply_status: String,
    pub private_reply_status: String,
}

pub struct Database {
    connection: Mutex<Connection>,
}

#[derive(Clone, Copy, Debug)]
pub enum DeliveryKind {
    PublicReply,
    PrivateReply,
}

pub struct DeliveryAction {
    pub comment_id: String,
    pub kind: DeliveryKind,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create database directory: {error}"))?;
        }

        let connection =
            Connection::open(path).map_err(|error| format!("open database: {error}"))?;
        #[cfg(unix)]
        if path != Path::new(":memory:") {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("secure database file permissions: {error}"))?;
        }
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("configure database: {error}"))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 CREATE TABLE IF NOT EXISTS matched_comments (
                     comment_id TEXT PRIMARY KEY,
                     account_id TEXT NOT NULL,
                     media_id TEXT NOT NULL,
                     commenter_id TEXT NOT NULL,
                     received_at INTEGER NOT NULL,
                     status TEXT NOT NULL DEFAULT 'awaiting_sender'
                 );
                 CREATE TABLE IF NOT EXISTS seen_comments (
                     comment_id TEXT PRIMARY KEY
                 );
                 CREATE TABLE IF NOT EXISTS poll_control (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS app_settings (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );",
            )
            .map_err(|error| format!("initialize database: {error}"))?;
        let public_status_migrated = ensure_column(&connection, "public_reply_status")?;
        let private_status_migrated = ensure_column(&connection, "private_reply_status")?;
        if public_status_migrated || private_status_migrated {
            connection
                .execute_batch(
                    "UPDATE matched_comments
                     SET public_reply_status = 'uncertain', private_reply_status = 'uncertain';",
                )
                .map_err(|error| format!("mark pre-existing queue rows as uncertain: {error}"))?;
        }
        connection
            .execute_batch(
                "UPDATE matched_comments
                 SET public_reply_status = 'uncertain'
                 WHERE public_reply_status = 'in_flight';
                 UPDATE matched_comments
                 SET private_reply_status = 'uncertain'
                 WHERE private_reply_status = 'in_flight';",
            )
            .map_err(|error| format!("recover interrupted deliveries: {error}"))?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn insert_matched(&self, comment: &MatchedComment) -> Result<bool, String> {
        let received_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock error: {error}"))?
            .as_secs() as i64;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let inserted = connection
            .execute(
                "INSERT INTO matched_comments
                    (comment_id, account_id, media_id, commenter_id, received_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(comment_id) DO NOTHING",
                params![
                    comment.comment_id,
                    comment.account_id,
                    comment.media_id,
                    comment.commenter_id,
                    received_at
                ],
            )
            .map_err(|error| format!("save matched comment: {error}"))?;
        Ok(inserted == 1)
    }

    pub fn ingest_poll(
        &self,
        comments: &[PolledComment],
        account_id: &str,
        keyword: &str,
    ) -> Result<PollResult, String> {
        let received_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock error: {error}"))?
            .as_secs() as i64;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("begin poll transaction: {error}"))?;
        let baseline_key = format!("baseline_complete:{account_id}");
        let baseline_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM poll_control WHERE key = ?1)",
                [&baseline_key],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| format!("read poll baseline: {error}"))?;
        let mut result = PollResult {
            baseline_created: !baseline_exists,
            new_comments: 0,
            matched_comments: 0,
        };
        for comment in comments {
            let fresh = transaction
                .execute(
                    "INSERT INTO seen_comments (comment_id) VALUES (?1)
                     ON CONFLICT(comment_id) DO NOTHING",
                    [&comment.comment_id],
                )
                .map_err(|error| format!("remember polled comment: {error}"))?
                == 1;
            if !fresh || !baseline_exists {
                continue;
            }
            result.new_comments += 1;
            if comment.is_owner
                || comment.commenter_id == account_id
                || !is_exact_keyword(&comment.text, keyword)
            {
                continue;
            }
            result.matched_comments += transaction
                .execute(
                    "INSERT INTO matched_comments
                        (comment_id, account_id, media_id, commenter_id, received_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(comment_id) DO NOTHING",
                    params![
                        comment.comment_id,
                        account_id,
                        comment.media_id,
                        comment.commenter_id,
                        received_at
                    ],
                )
                .map_err(|error| format!("queue polled comment: {error}"))?;
        }
        if !baseline_exists {
            transaction
                .execute(
                    "INSERT INTO poll_control (key, value) VALUES (?1, '1')",
                    [&baseline_key],
                )
                .map_err(|error| format!("save poll baseline: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("commit poll transaction: {error}"))?;
        Ok(result)
    }

    pub fn health_check(&self) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("database health check: {error}"))?;
        Ok(())
    }

    pub fn save_settings(&self, config: &Config) -> Result<(), String> {
        let serialized = serde_json::to_string(config)
            .map_err(|error| format!("serialize bot settings: {error}"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .execute(
                "INSERT INTO app_settings (key, value) VALUES ('current', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&serialized],
            )
            .map_err(|error| format!("save bot settings: {error}"))?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Option<Config>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let value = connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'current'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("load bot settings: {error}"))?;
        value
            .map(|serialized| {
                serde_json::from_str(&serialized)
                    .map_err(|error| format!("read saved bot settings: {error}"))
            })
            .transpose()
    }

    pub fn has_saved_settings(&self) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_settings WHERE key = 'current')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("check saved bot settings: {error}"))
    }

    pub fn save_deployment_target(&self, target: &DeploymentTarget) -> Result<(), String> {
        let serialized = serde_json::to_string(target)
            .map_err(|error| format!("serialize deployment target: {error}"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .execute(
                "INSERT INTO app_settings (key, value) VALUES ('deployment_target', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&serialized],
            )
            .map_err(|error| format!("save deployment target: {error}"))?;
        Ok(())
    }

    pub fn load_deployment_target(&self) -> Result<Option<DeploymentTarget>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let value = connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'deployment_target'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("load deployment target: {error}"))?;
        value
            .map(|serialized| {
                serde_json::from_str(&serialized)
                    .map_err(|error| format!("read saved deployment target: {error}"))
            })
            .transpose()
    }

    pub fn has_completed_delivery_for_account(&self, account_id: &str) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM matched_comments
                 WHERE account_id = ?1 AND public_reply_status = 'sent'
                   AND private_reply_status = 'sent')",
                [account_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("check completed delivery: {error}"))
    }

    pub fn recent_activity(&self, limit: u32) -> Result<Vec<RecentActivity>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let mut statement = connection
            .prepare(
                "SELECT received_at, public_reply_status, private_reply_status
                 FROM matched_comments
                 ORDER BY received_at DESC
                 LIMIT ?1",
            )
            .map_err(|error| format!("prepare recent activity query: {error}"))?;
        let rows = statement
            .query_map([limit], |row| {
                Ok(RecentActivity {
                    received_at: row.get(0)?,
                    public_reply_status: row.get(1)?,
                    private_reply_status: row.get(2)?,
                })
            })
            .map_err(|error| format!("read recent activity: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read recent activity row: {error}"))
    }

    pub fn queue_summary(&self) -> Result<QueueSummary, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .query_row(
                "SELECT
                    COALESCE(SUM(public_reply_status = 'pending'), 0),
                    COALESCE(SUM(public_reply_status = 'sent' AND private_reply_status = 'pending'), 0),
                    COALESCE(SUM(public_reply_status = 'sent' AND private_reply_status = 'sent'), 0),
                    COALESCE(SUM(public_reply_status IN ('failed', 'uncertain') OR private_reply_status IN ('failed', 'uncertain')), 0)
                 FROM matched_comments",
                [],
                |row| {
                    Ok(QueueSummary {
                        awaiting_public_reply: row.get::<_, i64>(0)?.max(0) as u64,
                        awaiting_private_reply: row.get::<_, i64>(1)?.max(0) as u64,
                        completed: row.get::<_, i64>(2)?.max(0) as u64,
                        needs_attention: row.get::<_, i64>(3)?.max(0) as u64,
                    })
                },
            )
            .map_err(|error| format!("read reply queue summary: {error}"))
    }

    #[cfg(test)]
    pub fn claim_next(&self) -> Result<Option<DeliveryAction>, String> {
        self.claim_next_for(None)
    }

    pub fn claim_next_for(
        &self,
        account_id: Option<&str>,
    ) -> Result<Option<DeliveryAction>, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("begin delivery transaction: {error}"))?;
        let row = transaction
            .query_row(
                "SELECT comment_id, public_reply_status
                 FROM matched_comments
                 WHERE (?1 IS NULL OR account_id = ?1)
                   AND (public_reply_status = 'pending'
                    OR (public_reply_status = 'sent' AND private_reply_status = 'pending'))
                 ORDER BY received_at
                 LIMIT 1",
                [account_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| format!("find pending delivery: {error}"))?;

        let Some((comment_id, public_status)) = row else {
            transaction
                .commit()
                .map_err(|error| format!("finish delivery transaction: {error}"))?;
            return Ok(None);
        };
        let (kind, column) = if public_status == "pending" {
            (DeliveryKind::PublicReply, "public_reply_status")
        } else {
            (DeliveryKind::PrivateReply, "private_reply_status")
        };
        transaction
            .execute(
                &format!(
                    "UPDATE matched_comments SET {column} = 'in_flight' WHERE comment_id = ?1"
                ),
                [&comment_id],
            )
            .map_err(|error| format!("claim delivery: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("finish delivery transaction: {error}"))?;

        Ok(Some(DeliveryAction { comment_id, kind }))
    }

    pub fn finish_delivery(&self, action: &DeliveryAction, status: &str) -> Result<(), String> {
        if !matches!(status, "sent" | "failed" | "uncertain") {
            return Err("invalid delivery status".to_owned());
        }
        let column = match action.kind {
            DeliveryKind::PublicReply => "public_reply_status",
            DeliveryKind::PrivateReply => "private_reply_status",
        };
        let connection = self
            .connection
            .lock()
            .map_err(|_| "database lock was poisoned".to_owned())?;
        connection
            .execute(
                &format!("UPDATE matched_comments SET {column} = ?1 WHERE comment_id = ?2"),
                params![status, action.comment_id],
            )
            .map_err(|error| format!("save delivery status: {error}"))?;
        Ok(())
    }
}

fn ensure_column(connection: &Connection, column: &str) -> Result<bool, String> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('matched_comments') WHERE name = ?1)",
            [column],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| format!("inspect database schema: {error}"))?;
    if exists {
        return Ok(false);
    }
    let statement = match column {
        "public_reply_status" => {
            "ALTER TABLE matched_comments ADD COLUMN public_reply_status TEXT NOT NULL DEFAULT 'pending'"
        }
        "private_reply_status" => {
            "ALTER TABLE matched_comments ADD COLUMN private_reply_status TEXT NOT NULL DEFAULT 'pending'"
        }
        _ => return Err("unsupported database migration".to_owned()),
    };
    connection
        .execute_batch(statement)
        .map_err(|error| format!("migrate database schema: {error}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{Database, DeliveryKind, DeploymentTarget, PolledComment};
    use crate::webhook::MatchedComment;
    use std::path::Path;

    #[test]
    fn sends_public_reply_before_private_reply() {
        let database = Database::open(Path::new(":memory:")).unwrap();
        let comment = MatchedComment {
            account_id: "123".into(),
            comment_id: "456".into(),
            media_id: "789".into(),
            commenter_id: "101".into(),
        };
        assert!(database.insert_matched(&comment).unwrap());
        assert!(!database.insert_matched(&comment).unwrap());

        let public = database.claim_next().unwrap().unwrap();
        assert!(matches!(public.kind, DeliveryKind::PublicReply));
        assert!(database.claim_next().unwrap().is_none());

        database.finish_delivery(&public, "sent").unwrap();
        let private = database.claim_next().unwrap().unwrap();
        assert!(matches!(private.kind, DeliveryKind::PrivateReply));
        database.finish_delivery(&private, "sent").unwrap();
        assert!(database.claim_next().unwrap().is_none());
        assert!(database.has_completed_delivery_for_account("123").unwrap());
        assert!(
            !database
                .has_completed_delivery_for_account("other")
                .unwrap()
        );
    }

    #[test]
    fn remembers_the_selected_vps_without_marking_reply_rules_saved() {
        let database = Database::open(Path::new(":memory:")).unwrap();
        let target = DeploymentTarget {
            host: "192.0.2.10".to_owned(),
            ssh_port: 2222,
            service_port: 12950,
            host_fingerprint: None,
        };
        database.save_deployment_target(&target).unwrap();
        let loaded = database.load_deployment_target().unwrap().unwrap();
        assert_eq!(loaded.host, target.host);
        assert_eq!(loaded.ssh_port, target.ssh_port);
        assert_eq!(loaded.service_port, target.service_port);
        assert!(!database.has_saved_settings().unwrap());
    }

    #[test]
    fn failed_public_reply_does_not_send_private_reply() {
        let database = Database::open(Path::new(":memory:")).unwrap();
        let comment = MatchedComment {
            account_id: "123".into(),
            comment_id: "456".into(),
            media_id: "789".into(),
            commenter_id: "101".into(),
        };
        database.insert_matched(&comment).unwrap();
        let public = database.claim_next().unwrap().unwrap();
        database.finish_delivery(&public, "failed").unwrap();
        assert!(database.claim_next().unwrap().is_none());
    }

    #[test]
    fn poll_baselines_existing_comments_and_queues_only_new_keyword_comments() {
        let database = Database::open(Path::new(":memory:")).unwrap();
        let comment = |id: &str, text: &str, commenter: &str| PolledComment {
            comment_id: id.into(),
            media_id: "99".into(),
            commenter_id: commenter.into(),
            is_owner: commenter == "owner",
            text: text.into(),
        };
        let old = comment("1", "бот", "other");
        let baseline = database.ingest_poll(&[old], "owner", "бот").unwrap();
        assert!(baseline.baseline_created);
        assert_eq!(baseline.matched_comments, 0);
        assert!(database.claim_next().unwrap().is_none());

        let second = database
            .ingest_poll(
                &[
                    comment("1", "бот", "other"),
                    comment("2", "БОТ!", "other"),
                    PolledComment {
                        comment_id: "3".into(),
                        media_id: "99".into(),
                        commenter_id: "scoped-owner-id".into(),
                        is_owner: true,
                        text: "бот".into(),
                    },
                    comment("4", "not a bot", "other"),
                ],
                "owner",
                "бот",
            )
            .unwrap();
        assert!(!second.baseline_created);
        assert_eq!(second.new_comments, 3);
        assert_eq!(second.matched_comments, 1);
        let action = database.claim_next().unwrap().unwrap();
        assert_eq!(action.comment_id, "2");
        assert!(matches!(action.kind, DeliveryKind::PublicReply));
    }
}

#[test]
fn migration_does_not_enqueue_comments_stored_by_an_older_version() {
    let path = std::env::temp_dir().join(format!(
        "ig-reply-bot-migration-{}.sqlite3",
        std::process::id()
    ));
    let legacy = rusqlite::Connection::open(&path).unwrap();
    legacy
        .execute_batch(
            "CREATE TABLE matched_comments (
                    comment_id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL,
                    media_id TEXT NOT NULL,
                    commenter_id TEXT NOT NULL,
                    received_at INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'awaiting_sender'
                 );
                 INSERT INTO matched_comments
                    (comment_id, account_id, media_id, commenter_id, received_at)
                 VALUES ('old-test-comment', '1', '2', '3', 1);",
        )
        .unwrap();
    drop(legacy);

    let database = Database::open(&path).unwrap();
    assert!(database.claim_next().unwrap().is_none());
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
}
