use crate::cache::Ttype;
use crate::settings::Settings;
use crate::types::CheckTrait;
use anyhow::bail;
use anyhow::Result;
use different::DiffSettings;
use log::debug;
use minijinja::Environment;
use std::fs;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::cache::Cache;
use crate::types::Checklist;
use crate::types::Statuses;

fn checklists_in_dir(path: &Path) -> Result<Vec<Checklist>> {
    let mut checklists = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            debug!("Reading '{}'", path.display());
            let checklist = Checklist::from_path(path)?;
            checklists.push(checklist);
        }
    }
    Ok(checklists)
}

// TODO: some sort of 'checklist ignore' directive for non-checklist toml files
fn discover_project_checklists(project_dir: &Path) -> Result<Vec<Checklist>> {
    let mut checklists = Vec::new();

    for name in [".checklists", "checklists", "checks", ".checks"] {
        let path = project_dir.join(name);
        if path.is_dir() {
            checklists.append(&mut checklists_in_dir(&path)?);
        }
    }

    for name in [".checklist.toml", "checklist.toml"] {
        let path = project_dir.join(name);
        if path.is_file() {
            let checklist = Checklist::from_path(path)?;
            checklists.push(checklist);
        }
    }

    Ok(checklists)
}

fn discover_remote_checklists(settings: &Settings, cache: &mut Cache) -> Result<Vec<Checklist>> {
    let mut checklists = Vec::new();

    for external in settings.external_checklists() {
        let url = external.url();
        let name = url.name();
        let hash = external.hash();
        let path = cache.get_or_dl_external_file(
            &name,
            url.to_string(),
            hash.cloned(),
            Ttype::Checklist,
        )?;

        let checklist = Checklist::from_path(path)?;
        checklists.push(checklist);
    }

    Ok(checklists)
}

fn discover_checklists(
    project_dir: &Path,
    user_checklists_dir: Option<PathBuf>,
    settings: &Settings,
    cache: &mut Cache,
) -> Result<Vec<Checklist>> {
    let mut checklists = Vec::new();

    checklists.append(&mut discover_remote_checklists(settings, cache)?);

    if let Some(user_checklists_dir) = user_checklists_dir {
        if !user_checklists_dir.is_dir() {
            bail!(
                "User checklists dir ({}) does not exist",
                user_checklists_dir.display()
            );
        }
        checklists.append(&mut checklists_in_dir(&user_checklists_dir)?);
    }

    checklists.append(&mut discover_project_checklists(project_dir)?);

    Ok(checklists)
}

fn add_template(template_env: &mut Environment, path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            add_template(template_env, &path)?;
        }
    } else if path.is_file() {
        let name = path.display().to_string();
        debug!("adding template {}", path.display());
        let contents = fs::read_to_string(path)?;
        template_env.add_template_owned(name, contents)?;
    } else {
        bail!(
            "Unsupported file type: {}, expected dir or regular file",
            path.display()
        );
    }

    Ok(())
}

#[derive(Debug)]
pub struct Project<'a> {
    root: PathBuf,
    cache: Cache,
    checklists: Vec<Checklist>,
    settings: Settings,
    diff_settings: DiffSettings,
    template_env: Environment<'a>,
    facts: HashMap<String, String>,
}

// TODO: need to refactor the whole discover templates and checklists thing. Its grown to be spaghetti

impl Project<'_> {
    pub fn new(
        dir: PathBuf,
        settings: Settings,
        diff_settings: DiffSettings,
        user_checklists_dir: PathBuf,
        user_templates_dir: PathBuf,
        cache_dir: PathBuf,
    ) -> Result<Self> {
        let project_name = dir.file_stem().unwrap().to_str().unwrap();

        let mut template_env = Environment::new();

        // TODO: cache should hash the templates, because if those have changed cache is no longer valid
        let user_checklists_dir = if settings.user_checklists() {
            // Register user templates
            if user_templates_dir.is_dir() {
                add_template(&mut template_env, &user_templates_dir)?;
            }

            Some(user_checklists_dir)
        } else {
            None
        };

        let mut facts = HashMap::new();

        let mut cache = match Cache::load(cache_dir.clone(), project_name.to_string())? {
            Some(cache) => {
                let cache = if *cache.facts() == facts {
                    cache
                } else {
                    // Facts are out of date, remove old cache entry and create new one
                    let cache_dir = cache.cache_dir();
                    fs::remove_dir_all(cache_dir)?; // TODO: make a method to remove the cache for DRY
                    Cache::new(
                        cache_dir.to_path_buf(),
                        project_name.to_string(),
                        facts.clone(),
                    )?
                };

                if settings.clear_cache() {
                    fs::remove_dir_all(&cache_dir)?;
                    Cache::new(cache_dir.clone(), project_name.to_string(), facts.clone())?
                } else {
                    cache
                }
            }
            None => Cache::new(cache_dir.clone(), project_name.to_string(), facts.clone())?,
        };

        for template in settings.external_templates() {
            let url = template.url();
            let name = url.name();
            let hash = template.hash();
            let path = cache.get_or_dl_external_file(
                &name,
                url.to_string(),
                hash.cloned(),
                Ttype::Template,
            )?;
            let path = path.canonicalize()?;
            add_template(&mut template_env, &path)?;
        }

        let checklists = discover_checklists(&dir, user_checklists_dir, &settings, &mut cache)?;
        for checklist in &checklists {
            let name = checklist.name()?;
            let path = checklist.path();
            for fact in checklist.facts() {
                for requirement in fact.requirements() {
                    let status =
                        requirement.do_check(&diff_settings, &template_env, path, &facts)?;

                    if status.is_failure() {
                        bail!("{status}");
                    }
                }

                let k = fact.key();
                let v = fact.value(&facts)?;
                debug!("Found fact '{k}'='{v}' for checklist '{name}'");
                facts.insert(k, v);
            }

            for template in &checklist.templates() {
                add_template(&mut template_env, template)?;
            }
        }

        Ok(Self {
            root: dir,
            cache,
            checklists,
            settings,
            diff_settings,
            template_env,
            facts,
        })
    }

    pub fn run_checks(&mut self) -> Result<Statuses> {
        let mut statuses = Statuses::new();

        for checklist in &self.checklists {
            let checklist_path = checklist.path();
            let checklist_name = checklist.name()?;
            debug!("Running with checklist {checklist_name}");

            for check in checklist.checks() {
                let check_name = check.description();
                debug!("Running check: {check_name}");

                let status = if self.settings.no_read_cache() {
                    match self.cache.get(check)? {
                        Some(status) => {
                            debug!("Check '{check_name}' status pulled from cache");
                            status
                        }
                        None => {
                            let status = check.do_check(
                                &self.diff_settings,
                                &self.template_env,
                                checklist_path,
                                &self.facts,
                            )?;
                            if !self.settings.no_write_cache() {
                                self.cache.insert(check.clone(), status.clone())?;
                            }
                            status
                        }
                    }
                } else {
                    let status = check.do_check(
                        &self.diff_settings,
                        &self.template_env,
                        checklist_path,
                        &self.facts,
                    )?;
                    if !self.settings.no_write_cache() {
                        self.cache.insert(check.clone(), status.clone())?;
                    }
                    status
                };

                statuses.insert(checklist_path.to_path_buf(), check_name.to_string(), status);
            }
        }

        self.cache.save()?;
        Ok(statuses)
    }
}
