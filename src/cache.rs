use crate::types::Check;
use crate::types::CheckType;
use crate::types::Status;
use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use blake3::Hasher;
use log::debug;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::io::{BufReader, Read};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

// TODO: need a mechanism for garbage collection

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();

    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct PathMap {
    /// Map file path to md5
    map: HashMap<PathBuf, String>,
}

impl PathMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: PathBuf) -> Result<()> {
        let hash = hash_file(&path)?;
        self.map.insert(path, hash);
        Ok(())
    }

    pub fn get(&self, path: &Path) -> Option<&String> {
        self.map.get(path)
    }
}

fn cache_files(dir: &Path, project_name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let path_file_name = format!("{project_name}-paths.json");
    let path_file = dir.join(path_file_name);
    let check_file_name = format!("{project_name}-checks.json");
    let check_file = dir.join(check_file_name);
    let facts_file_name = format!("{project_name}-facts.json");
    let facts_file = dir.join(facts_file_name);
    (path_file, check_file, facts_file)
}

fn hash_check(check: &Check) -> Result<String> {
    let json = serde_json::to_vec(check)?;
    let mut hasher = Hasher::new();
    hasher.update(&json);
    let hash = hasher.finalize();
    let encoded = URL_SAFE_NO_PAD.encode(hash.as_bytes());
    Ok(encoded)
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckMap {
    /// Map Check to status
    map: HashMap<String, Status>,
}

impl CheckMap {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn get(&self, check: &Check) -> Result<Option<Status>> {
        let hash = hash_check(check)?;
        let status = self.map.get(&hash).map(|x| x.clone());
        Ok(status)
    }

    fn insert(&mut self, check: Check, status: Status) -> Result<()> {
        let hash = hash_check(&check)?;
        self.map.insert(hash, status);
        Ok(())
    }
}

// TODO: rewrite with an sqlite table
#[derive(Debug)]
pub struct Cache {
    cache_dir: PathBuf,
    path_map: PathMap,
    check_map: CheckMap,
    project_name: String,
    facts: HashMap<String, String>,
}

impl Cache {
    pub fn new(cache_dir: PathBuf, project_name: String, facts: HashMap<String, String>) -> Self {
        let cache_dir = cache_dir.join(&project_name);
        Self {
            cache_dir,
            check_map: CheckMap::new(),
            path_map: PathMap::new(),
            project_name,
            facts,
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn facts(&self) -> &HashMap<String, String> {
        &self.facts
    }

    pub fn load(cache_dir: PathBuf, project_name: String) -> Result<Option<Self>> {
        let cache_dir = cache_dir.join(&project_name);

        let (path_cache_file, check_cache_file, facts_cache_file) =
            cache_files(&cache_dir, &project_name);
        debug!(
            "Loading cache files: {}, {}, {}",
            path_cache_file.display(),
            check_cache_file.display(),
            facts_cache_file.display()
        );

        if !(path_cache_file.is_file() && check_cache_file.is_file()) {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path_cache_file)?;
        let path_map: PathMap = serde_json::from_str(&contents)?;

        let contents = fs::read_to_string(&check_cache_file)?;
        let check_map: CheckMap = serde_json::from_str(&contents)?;

        let contents = fs::read_to_string(&facts_cache_file)?;
        let facts: HashMap<String, String> = serde_json::from_str(&contents)?;

        Ok(Some(Self {
            path_map,
            check_map,
            cache_dir,
            project_name,
            facts,
        }))
    }

    pub fn save(&self) -> Result<()> {
        if !self.cache_dir.is_dir() {
            fs::create_dir_all(&self.cache_dir)?;
        }

        let (path_cache_file, check_cache_file, facts_cache_file) =
            cache_files(&self.cache_dir, &self.project_name);
        debug!(
            "Saving cache files: {}, {}, {}",
            path_cache_file.display(),
            check_cache_file.display(),
            facts_cache_file.display()
        );

        let mut f = File::create(&path_cache_file)?;
        let contents = serde_json::to_string(&self.path_map)?;
        write!(f, "{contents}")?;

        let mut f = File::create(&check_cache_file)?;
        let contents = serde_json::to_string(&self.check_map)?;
        write!(f, "{contents}")?;

        let mut f = File::create(&facts_cache_file)?;
        let contents = serde_json::to_string(&self.facts)?;
        write!(f, "{contents}")?;

        Ok(())
    }

    pub fn get(&self, check: &Check) -> Result<Option<Status>> {
        let check_name = check.description();
        debug!("Checking cache for '{check_name}'");

        let status = match check.ttype() {
            CheckType::File(f) => {
                let path = f.path();

                match &self.path_map.get(path) {
                    Some(old_hash) => {
                        // Check if file has changed
                        let new_hash = hash_file(&path)?;
                        if **old_hash == new_hash {
                            match &self.check_map.get(check)? {
                                Some(status) => Some(status).cloned(),
                                None => None,
                            }
                        } else {
                            // TODO: remove old entry from path_map
                            None
                        }
                    }
                    None => None,
                }
            }
            CheckType::Directory(d) => {
                // TODO
                None
            }
            CheckType::Command(c) => {
                // TODO
                None
            }
            CheckType::Http(h) => {
                // TODO
                None
            }
            CheckType::VarSet(v) => {
                // Dont ever cache
                None
            }
        };
        Ok(status)
    }

    pub fn insert(&mut self, check: Check, mut status: Status) -> Result<()> {
        status.mark_as_cached();

        let check_name = check.description();
        debug!("Inserting status ({status}) of '{check_name}' into cache");

        match check.ttype() {
            CheckType::File(f) => {
                let path = f.path();
                self.path_map.insert(path.to_path_buf())?;
                self.check_map.insert(check, status)?;
            }
            _ => {
                // TODO
            }
        }

        Ok(())
    }
}
