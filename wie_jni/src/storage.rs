//! Filesystem + save-data (RMS/record store) backends for Android.
//!
//! Logic is a straight port of `wie_cli`'s `CliFilesystem` / `DatabaseRepository`
//! (see wie_cli/src/{filesystem,database}.rs) with `directories::ProjectDirs`
//! swapped for a path handed to us from Kotlin (`Context.getFilesDir()`),
//! since `directories` doesn't resolve anything sensible under Android.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use wie_backend::RecordId;

fn sanitize_aid(aid: &str) -> Option<String> {
    let sanitized: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        None
    } else {
        Some(sanitized)
    }
}

fn normalize(path: &str) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(c) => normalized.push(c),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if normalized.as_os_str().is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub struct AndroidFilesystem {
    base_path: PathBuf,
}

impl AndroidFilesystem {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn path_for(&self, aid: &str, path: &str) -> Option<PathBuf> {
        let aid = sanitize_aid(aid)?;
        let normalized = normalize(path)?;
        Some(self.base_path.join(aid).join("fs").join(normalized))
    }
}

#[async_trait::async_trait]
impl wie_backend::Filesystem for AndroidFilesystem {
    async fn exists(&self, aid: &str, path: &str) -> bool {
        self.path_for(aid, path).is_some_and(|p| p.metadata().is_ok_and(|m| m.is_file()))
    }

    async fn size(&self, aid: &str, path: &str) -> Option<usize> {
        let path = self.path_for(aid, path)?;
        let md = path.metadata().ok()?;
        md.is_file().then_some(md.len() as usize)
    }

    async fn read(&self, aid: &str, path: &str, offset: usize, count: usize, buf: &mut [u8]) -> Option<usize> {
        let disk_path = self.path_for(aid, path)?;

        let mut file = match OpenOptions::new().read(true).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return None;
                }
                tracing::warn!(aid, path, %err, "read: open failed");
                return None;
            }
        };

        let size = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
        if offset >= size {
            return Some(0);
        }

        if let Err(err) = file.seek(SeekFrom::Start(offset as u64)) {
            tracing::warn!(aid, path, %err, "read: seek failed");
            return Some(0);
        }

        let to_read = core::cmp::min(count, size - offset);
        let slice = &mut buf[..to_read];
        match file.read_exact(slice) {
            Ok(()) => Some(to_read),
            Err(err) => {
                tracing::warn!(aid, path, %err, "read: IO error");
                Some(0)
            }
        }
    }

    async fn write(&self, aid: &str, path: &str, offset: usize, data: &[u8]) -> usize {
        let Some(disk_path) = self.path_for(aid, path) else {
            return 0;
        };

        if let Some(parent) = disk_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            tracing::warn!(aid, path, %err, "write: create parent dir failed");
            return 0;
        }

        let mut file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                tracing::warn!(aid, path, %err, "write: open failed");
                return 0;
            }
        };

        let current_size = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
        if offset > current_size
            && let Err(err) = file.set_len(offset as u64)
        {
            tracing::warn!(aid, path, %err, "write: set_len extend failed");
            return 0;
        }

        if let Err(err) = file.seek(SeekFrom::Start(offset as u64)) {
            tracing::warn!(aid, path, %err, "write: seek failed");
            return 0;
        }

        match file.write_all(data) {
            Ok(()) => data.len(),
            Err(err) => {
                tracing::warn!(aid, path, %err, "write: write_all failed");
                0
            }
        }
    }

    async fn truncate(&self, aid: &str, path: &str, len: usize) {
        let Some(disk_path) = self.path_for(aid, path) else { return };

        if let Some(parent) = disk_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            tracing::warn!(aid, path, %err, "truncate: create parent dir failed");
            return;
        }

        let file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                tracing::warn!(aid, path, %err, "truncate: open failed");
                return;
            }
        };

        if let Err(err) = file.set_len(len as u64) {
            tracing::warn!(aid, path, %err, "truncate: set_len failed");
        }
    }
}

pub struct AndroidDatabaseRepository {
    base_path: PathBuf,
}

impl AndroidDatabaseRepository {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn get_path_for_database(&self, name: &str, app_id: &str) -> PathBuf {
        let app_id = sanitize_aid(app_id).unwrap_or_else(|| "_".to_string());
        let normalized_name = normalize(name).unwrap_or_else(|| PathBuf::from("_"));
        self.base_path.join(app_id).join("db").join(normalized_name)
    }
}

#[async_trait::async_trait]
impl wie_backend::DatabaseRepository for AndroidDatabaseRepository {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn wie_backend::Database> {
        let path = self.get_path_for_database(name, app_id);
        Box::new(AndroidDatabase::new(path).unwrap())
    }

    async fn exists(&self, name: &str, app_id: &str) -> bool {
        self.get_path_for_database(name, app_id).exists()
    }

    async fn delete(&self, name: &str, app_id: &str) -> bool {
        let path = self.get_path_for_database(name, app_id);
        match fs::remove_dir_all(path) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                tracing::warn!("Failed to delete database: {e}");
                false
            }
        }
    }
}

pub struct AndroidDatabase {
    base_path: PathBuf,
}

impl AndroidDatabase {
    pub fn new(base_path: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&base_path)?;
        Ok(Self { base_path })
    }

    fn find_empty_record_id(&self) -> RecordId {
        let mut record_id = 1; // XXX midp requires first record to be 1
        loop {
            if !self.base_path.join(record_id.to_string()).exists() {
                return record_id;
            }
            record_id += 1;
        }
    }

    fn get_path_for_record(&self, id: RecordId) -> PathBuf {
        self.base_path.join(id.to_string())
    }
}

#[async_trait::async_trait]
impl wie_backend::Database for AndroidDatabase {
    async fn next_id(&self) -> RecordId {
        self.find_empty_record_id()
    }

    async fn add(&mut self, data: &[u8]) -> RecordId {
        let id = self.find_empty_record_id();
        let path = self.get_path_for_record(id);
        fs::write(path, data).unwrap();
        id
    }

    async fn get(&self, id: RecordId) -> Option<Vec<u8>> {
        fs::read(self.get_path_for_record(id)).ok()
    }

    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool {
        fs::write(self.get_path_for_record(id), data).is_ok()
    }

    async fn delete(&mut self, id: RecordId) -> bool {
        fs::remove_file(self.get_path_for_record(id)).is_ok()
    }

    async fn get_record_ids(&self) -> Vec<RecordId> {
        fs::read_dir(&self.base_path)
            .unwrap()
            .filter(|x| x.as_ref().unwrap().path().is_file())
            .map(|x| x.unwrap().file_name().to_str().unwrap().parse().unwrap())
            .collect()
    }
}
