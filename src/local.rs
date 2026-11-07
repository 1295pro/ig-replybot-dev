use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::config::Config;

pub struct LocalLaunch {
    pub directory: PathBuf,
    pub password: String,
    pub open_browser: bool,
}

impl LocalLaunch {
    pub fn prepare(directory: Option<PathBuf>, open_browser: bool) -> Result<Self, String> {
        let directory = directory.map_or_else(default_directory, Ok)?;
        fs::create_dir_all(&directory)
            .map_err(|error| format!("create local app folder: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("secure local app folder: {error}"))?;
        }
        let path = directory.join("admin-password.txt");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let password = match options.open(&path) {
            Ok(mut file) => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes)
                    .map_err(|error| format!("generate local panel password: {error}"))?;
                let password = hex::encode(bytes);
                file.write_all(password.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|error| format!("save local panel password: {error}"))?;
                password
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                fs::read_to_string(&path)
                    .map_err(|error| format!("read local panel password: {error}"))?
                    .trim()
                    .to_owned()
            }
            Err(error) => return Err(format!("create local panel password: {error}")),
        };
        if password.len() != 64 || !password.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "Invalid password file at {}. Restore it from your backup before starting.",
                path.display()
            ));
        }
        Ok(Self {
            directory,
            password,
            open_browser,
        })
    }

    pub fn config(&self) -> Config {
        Config {
            bind_addr: "127.0.0.1:3000".parse().expect("valid local address"),
            admin_password: self.password.clone(),
            database_path: self.directory.join("bot.sqlite3"),
            webhook_verify_token: String::new(),
            app_secret: String::new(),
            keyword: "bot".to_owned(),
            automation_enabled: false,
            poll_enabled: true,
            poll_interval_seconds: 60,
            ig_user_id: None,
            ig_username: String::new(),
            ig_access_token: None,
            api_version: "v26.0".to_owned(),
            public_reply_text: "Thanks for commenting!".to_owned(),
            private_reply_text: "Here is the information you asked for.".to_owned(),
        }
    }

    pub async fn already_running(&self) -> bool {
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return false;
        };
        let Ok(response) = client
            .get("http://127.0.0.1:3000/api/admin/status")
            .bearer_auth(&self.password)
            .send()
            .await
        else {
            return false;
        };
        response.status().is_success()
            && response
                .json::<serde_json::Value>()
                .await
                .is_ok_and(|body| {
                    body["configured"].is_object() && body["automation_enabled"].is_boolean()
                })
    }

    pub fn open(&self) {
        if !self.open_browser {
            return;
        }
        // Fragments are consumed by the browser, never sent in HTTP requests.
        let url = format!("http://127.0.0.1:3000/#local-token={}", self.password);
        #[cfg(target_os = "windows")]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut command = Command::new("rundll32.exe");
            command
                .arg("url.dll,FileProtocolHandler")
                .creation_flags(0x08000000);
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = Command::new("open");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = Command::new("xdg-open");
        if command
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_err()
        {
            eprintln!(
                "Open http://127.0.0.1:3000/ and sign in with the password in {}",
                self.directory.join("admin-password.txt").display()
            );
        }
    }
}

fn default_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let directory =
        env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("IG Reply Bot"));
    #[cfg(target_os = "macos")]
    let directory = env::var_os("HOME")
        .map(|path| PathBuf::from(path).join("Library/Application Support/IG Reply Bot"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let directory = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/share")))
        .map(|path| path.join("ig-reply-bot"));
    directory.ok_or_else(|| "Cannot find your user data folder. Supply --data-dir PATH.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::LocalLaunch;

    #[test]
    fn local_credentials_survive_relaunch_and_invalid_files_are_rejected() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let directory = std::env::temp_dir().join(format!("ig-local-{}", hex::encode(random)));
        let first = LocalLaunch::prepare(Some(directory.clone()), false).unwrap();
        let second = LocalLaunch::prepare(Some(directory.clone()), false).unwrap();
        assert_eq!(first.password, second.password);
        assert!(!first.config().automation_enabled);
        assert!(first.config().bind_addr.ip().is_loopback());
        std::fs::write(directory.join("admin-password.txt"), "damaged").unwrap();
        assert!(LocalLaunch::prepare(Some(directory.clone()), false).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
