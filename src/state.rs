use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

use crate::model::AppState;

const STATE_DIR: &str = ".launch-code";
const STATE_FILE: &str = "state.json";
const STATE_LOCK_FILE: &str = "state.lock";

#[derive(Debug, Error)]
pub enum StateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn state_dir_path(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    pub fn root_path(&self) -> &Path {
        &self.root
    }

    pub fn state_file_path(&self) -> PathBuf {
        self.state_dir_path().join(STATE_FILE)
    }

    pub fn state_lock_path(&self) -> PathBuf {
        self.state_dir_path().join(STATE_LOCK_FILE)
    }

    pub fn load(&self) -> Result<AppState, StateError> {
        let lock_file = self.open_lock_file()?;
        FileExt::lock_shared(&lock_file)?;
        let result = self.load_unlocked();
        let _ = FileExt::unlock(&lock_file);
        result
    }

    fn load_unlocked(&self) -> Result<AppState, StateError> {
        let path = self.state_file_path();
        if !path.exists() {
            return Ok(AppState::default());
        }

        let data = fs::read_to_string(path)?;
        if data.trim().is_empty() {
            return Ok(AppState::default());
        }
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, state: &AppState) -> Result<(), StateError> {
        let lock_file = self.open_lock_file()?;
        FileExt::lock_exclusive(&lock_file)?;
        let result = self.save_unlocked(state);
        let _ = FileExt::unlock(&lock_file);
        result
    }

    pub fn update<F, R, E>(&self, mutate: F) -> Result<R, E>
    where
        F: FnOnce(&mut AppState) -> Result<R, E>,
        E: From<StateError>,
    {
        let lock_file = self.open_lock_file().map_err(E::from)?;
        FileExt::lock_exclusive(&lock_file)
            .map_err(StateError::from)
            .map_err(E::from)?;

        let result = (|| -> Result<R, E> {
            let mut state = self.load_unlocked().map_err(E::from)?;
            let output = mutate(&mut state)?;
            self.save_unlocked(&state).map_err(E::from)?;
            Ok(output)
        })();

        let _ = FileExt::unlock(&lock_file);
        result
    }

    fn save_unlocked(&self, state: &AppState) -> Result<(), StateError> {
        let state_dir = self.state_dir_path();
        fs::create_dir_all(&state_dir)?;
        let state_path = self.state_file_path();
        let tmp_path = state_path.with_extension("json.tmp");
        let payload = serde_json::to_string_pretty(state)?;
        {
            let mut tmp_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)?;
            tmp_file.write_all(payload.as_bytes())?;
            tmp_file.sync_all()?;
        }
        fs::rename(&tmp_path, &state_path)?;
        sync_state_dir(&state_dir)?;
        Ok(())
    }

    fn open_lock_file(&self) -> Result<std::fs::File, StateError> {
        fs::create_dir_all(self.state_dir_path())?;
        let lock_path = self.state_lock_path();
        Ok(OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(lock_path)?)
    }
}

fn sync_state_dir(path: &Path) -> Result<(), StateError> {
    #[cfg(unix)]
    {
        let dir = OpenOptions::new().read(true).open(path)?;
        dir.sync_all()?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}
