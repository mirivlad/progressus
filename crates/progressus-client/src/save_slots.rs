use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use progressus_app::{Application, SaveMetadata};

pub(crate) const SAVE_SLOT_COUNT: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SaveSlot(u8);

impl SaveSlot {
    pub(crate) const fn all() -> [Self; SAVE_SLOT_COUNT as usize] {
        [Self(1), Self(2), Self(3)]
    }

    pub(crate) const fn number(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SaveSlotState {
    Empty,
    Ready(SaveMetadata),
    Invalid(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SaveSlotInfo {
    pub(crate) slot: SaveSlot,
    pub(crate) state: SaveSlotState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SaveNotice {
    Saved(SaveSlot),
    Loaded(SaveSlot),
    Error(String),
}

#[derive(Debug)]
pub(crate) struct SaveStore {
    root: PathBuf,
    slots: Vec<SaveSlotInfo>,
    revision: u64,
    notice: Option<SaveNotice>,
}

impl Default for SaveStore {
    fn default() -> Self {
        let root = default_save_root();
        let mut store = Self {
            root,
            slots: Vec::new(),
            revision: 0,
            notice: None,
        };
        store.refresh();
        store
    }
}

impl SaveStore {
    #[cfg(test)]
    pub(crate) fn at(root: PathBuf) -> Self {
        let mut store = Self {
            root,
            slots: Vec::new(),
            revision: 0,
            notice: None,
        };
        store.refresh();
        store
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn slots(&self) -> &[SaveSlotInfo] {
        &self.slots
    }

    pub(crate) fn notice(&self) -> Option<&SaveNotice> {
        self.notice.as_ref()
    }

    pub(crate) fn set_notice(&mut self, notice: SaveNotice) {
        self.notice = Some(notice);
        self.bump_revision();
    }

    pub(crate) fn save_bytes(
        &mut self,
        slot: SaveSlot,
        bytes: &[u8],
    ) -> Result<(), SaveStorageError> {
        fs::create_dir_all(&self.root).map_err(|source| SaveStorageError::Io {
            action: "create save directory",
            path: self.root.clone(),
            source,
        })?;
        self.repair_slot(slot)?;
        let target = self.slot_path(slot);
        let temporary = self.temporary_path(slot);
        let backup = self.backup_path(slot);

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|source| SaveStorageError::Io {
                action: "create temporary save",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| SaveStorageError::Io {
                action: "write temporary save",
                path: temporary.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| SaveStorageError::Io {
            action: "sync temporary save",
            path: temporary.clone(),
            source,
        })?;
        drop(file);

        if target.exists() {
            remove_if_exists(&backup)?;
            fs::rename(&target, &backup).map_err(|source| SaveStorageError::Io {
                action: "move previous save to backup",
                path: target.clone(),
                source,
            })?;
        }
        if let Err(source) = fs::rename(&temporary, &target) {
            if backup.exists() && !target.exists() {
                let _ = fs::rename(&backup, &target);
            }
            return Err(SaveStorageError::Io {
                action: "install new save",
                path: target,
                source,
            });
        }
        remove_if_exists(&backup)?;
        self.refresh();
        Ok(())
    }

    pub(crate) fn load_bytes(&mut self, slot: SaveSlot) -> Result<Vec<u8>, SaveStorageError> {
        self.repair_slot(slot)?;
        let path = self.slot_path(slot);
        fs::read(&path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                SaveStorageError::Empty(slot)
            } else {
                SaveStorageError::Io {
                    action: "read save",
                    path,
                    source,
                }
            }
        })
    }

    pub(crate) fn refresh(&mut self) {
        self.slots = SaveSlot::all()
            .into_iter()
            .map(|slot| SaveSlotInfo {
                slot,
                state: self.scan_slot(slot),
            })
            .collect();
        self.bump_revision();
    }

    fn scan_slot(&mut self, slot: SaveSlot) -> SaveSlotState {
        if let Err(error) = self.repair_slot(slot) {
            return SaveSlotState::Invalid(error.to_string());
        }
        let path = self.slot_path(slot);
        match fs::read(&path) {
            Ok(bytes) => match Application::save_metadata(&bytes) {
                Ok(metadata) => SaveSlotState::Ready(metadata),
                Err(error) => SaveSlotState::Invalid(error.to_string()),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => SaveSlotState::Empty,
            Err(error) => SaveSlotState::Invalid(
                SaveStorageError::Io {
                    action: "scan save",
                    path,
                    source: error,
                }
                .to_string(),
            ),
        }
    }

    fn repair_slot(&self, slot: SaveSlot) -> Result<(), SaveStorageError> {
        let target = self.slot_path(slot);
        let temporary = self.temporary_path(slot);
        let backup = self.backup_path(slot);
        if !target.exists() && backup.exists() {
            fs::rename(&backup, &target).map_err(|source| SaveStorageError::Io {
                action: "restore interrupted save backup",
                path: backup.clone(),
                source,
            })?;
        } else if target.exists() && backup.exists() {
            remove_if_exists(&backup)?;
        }
        remove_if_exists(&temporary)
    }

    fn slot_path(&self, slot: SaveSlot) -> PathBuf {
        self.root.join(format!("slot-{}.json", slot.number()))
    }

    fn temporary_path(&self, slot: SaveSlot) -> PathBuf {
        self.root.join(format!(".slot-{}.tmp", slot.number()))
    }

    fn backup_path(&self, slot: SaveSlot) -> PathBuf {
        self.root.join(format!(".slot-{}.bak", slot.number()))
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

fn remove_if_exists(path: &Path) -> Result<(), SaveStorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SaveStorageError::Io {
            action: "remove stale save file",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn default_save_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
        return PathBuf::from(root).join("Progressus").join("saves");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Progressus")
            .join("saves");
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(root) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(root).join("progressus").join("saves");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("progressus")
                .join("saves");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("progressus-saves")
}

#[derive(Debug)]
pub(crate) enum SaveStorageError {
    Empty(SaveSlot),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl Display for SaveStorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(slot) => write!(formatter, "save slot {} is empty", slot.number()),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "{action} at {}: {source}", path.display()),
        }
    }
}

impl Error for SaveStorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Empty(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::{Command, NewGameOptions, SimulationTick, WorldSeed};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "progressus-save-store-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn slots_round_trip_metadata_and_recover_interrupted_backup() {
        let root = temporary_root();
        let slot = SaveSlot::all()[0];
        let mut application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(91),
        })
        .unwrap();
        application
            .execute(Command::AdvanceTicks { count: 12 })
            .unwrap();
        let bytes = application.save_json().unwrap();
        let mut store = SaveStore::at(root.clone());
        store.save_bytes(slot, &bytes).unwrap();
        assert_eq!(
            store.slots()[0].state,
            SaveSlotState::Ready(Application::save_metadata(&bytes).unwrap())
        );
        assert_eq!(store.load_bytes(slot).unwrap(), bytes);

        let target = root.join("slot-1.json");
        let backup = root.join(".slot-1.bak");
        fs::rename(&target, &backup).unwrap();
        store.refresh();
        assert!(target.exists());
        assert!(!backup.exists());
        let metadata = match &store.slots()[0].state {
            SaveSlotState::Ready(metadata) => *metadata,
            other => panic!("expected recovered slot, got {other:?}"),
        };
        assert_eq!(metadata.tick, SimulationTick::new(12));
        let _ = fs::remove_dir_all(root);
    }
}
