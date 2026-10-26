use std::{collections::BTreeMap, path::PathBuf};

/// Private user configuration takes precedence over legacy bundled files.
/// Release builds never discover configuration through the working directory.
fn paths(
    config_dir: Option<PathBuf>,
    home: Option<PathBuf>,
    executable: Option<PathBuf>,
    development: bool,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config) = config_dir {
        paths.push(config.join("Meetily/.env"));
    }
    if let Some(home) = home {
        paths.push(home.join(".meetily.env"));
    }
    if development {
        paths.extend(["../../.env", "../.env", ".env", "frontend/.env"].map(PathBuf::from));
    }
    // Preserve compatibility with older installations while new bundles omit .env.
    if let Some(parent) = executable.as_deref().and_then(|p| p.parent()) {
        paths.push(parent.join(".env"));
        paths.push(parent.join("../Resources/.env"));
    }
    paths
}

fn resolve(paths: &[PathBuf], mut values: BTreeMap<String, String>) -> BTreeMap<String, String> {
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let entries = match dotenvy::from_path_iter(path) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
            Err(_) => {
                eprintln!("runtime_config_invalid: cannot read a configuration file");
                continue;
            }
        };
        match entries {
            Ok(entries) => {
                for (key, value) in entries {
                    values.entry(key).or_insert(value);
                }
            }
            // Parser errors can contain secret values, so do not log their contents.
            Err(_) => eprintln!("runtime_config_invalid: cannot parse a configuration file"),
        }
    }
    values
}

pub fn load() {
    let paths = paths(
        dirs::config_dir(),
        dirs::home_dir(),
        std::env::current_exe().ok(),
        cfg!(debug_assertions),
    );
    for (key, value) in resolve(&paths, std::env::vars().collect()) {
        std::env::set_var(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_configuration_is_independent_of_launch_directory() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let executable = dir.path().join("bundle/MacOS/meetily");
        let candidates = paths(
            Some(config.clone()),
            Some(dir.path().join("home")),
            Some(executable),
            false,
        );
        assert!(candidates.iter().all(|p| p.is_absolute()));
        assert_eq!(candidates[0], config.join("Meetily/.env"));
        std::fs::create_dir_all(config.join("Meetily")).unwrap();
        std::fs::write(
            &candidates[0],
            "MEETILY_RESTORE_WEBHOOK_URL=https://archive.example.test/restore\n",
        )
        .unwrap();
        let values = resolve(&candidates, BTreeMap::new());
        assert_eq!(
            values["MEETILY_RESTORE_WEBHOOK_URL"],
            "https://archive.example.test/restore"
        );
    }

    #[test]
    fn explicit_environment_and_user_settings_win_over_legacy_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user.env");
        let legacy = dir.path().join("legacy.env");
        std::fs::write(
            &user,
            "MEETILY_LANGUAGE=fr\nMEETILY_N8N_X_API_KEY=user-key\n",
        )
        .unwrap();
        std::fs::write(
            &legacy,
            "MEETILY_LANGUAGE=en\nMEETILY_N8N_X_API_KEY=old-key\nMEETILY_UPLOAD_TIMEOUT=60\n",
        )
        .unwrap();
        let values = resolve(
            &[user, legacy],
            BTreeMap::from([("MEETILY_N8N_X_API_KEY".into(), "process-key".into())]),
        );
        assert_eq!(values["MEETILY_LANGUAGE"], "fr");
        assert_eq!(values["MEETILY_N8N_X_API_KEY"], "process-key");
        assert_eq!(values["MEETILY_UPLOAD_TIMEOUT"], "60");
    }

    #[test]
    fn malformed_configuration_does_not_apply_partial_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.env");
        std::fs::write(&path, "MEETILY_LANGUAGE=fr\nINVALID='unterminated\n").unwrap();
        assert!(resolve(&[path], BTreeMap::new()).is_empty());
    }
}
