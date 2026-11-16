//! Profile storage in the original app's ProfilesV2 layout:
//!
//! ```text
//! ProfilesV2/<PROFILE-UUID>.sdProfile/manifest.json
//! ProfilesV2/<PROFILE-UUID>.sdProfile/Profiles/<page dir>/manifest.json
//! ProfilesV2/<PROFILE-UUID>.sdProfile/Profiles/<page dir>/Images/*.png
//! ```
//!
//! Top-level pages are listed in the profile manifest; every other page
//! directory is a folder opened by a `profile.openchild` action.

use crate::catalog::{self, DeviceSpec};
use crate::model::*;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: Uuid,
    pub dir: PathBuf,
    pub manifest: ProfileManifest,
    pub pages: HashMap<Uuid, PageData>,
}

impl Profile {
    pub fn page_dir(&self, page: &Uuid) -> PathBuf {
        self.dir.join("Profiles").join(page_dir_name(page))
    }

    pub fn top_pages(&self) -> Vec<Uuid> {
        self.manifest.pages.pages.iter().filter_map(|p| p.parse().ok()).collect()
    }

    pub fn is_folder(&self, page: &Uuid) -> bool {
        !self.top_pages().contains(page)
    }

    pub fn current_page(&self) -> Option<Uuid> {
        let tops = self.top_pages();
        self.manifest
            .pages
            .current
            .parse()
            .ok()
            .filter(|p| tops.contains(p))
            .or_else(|| self.manifest.pages.default.parse().ok().filter(|p| tops.contains(p)))
            .or_else(|| tops.first().copied())
    }

    pub fn spec(&self) -> Option<&'static DeviceSpec> {
        catalog::spec_for_model(&self.manifest.device.model)
    }

    /// The folder (or page) that contains an `openchild` action pointing at `folder`.
    pub fn parent_of(&self, folder: &Uuid) -> Option<Uuid> {
        let target = folder.to_string();
        for (id, page) in &self.pages {
            for c in &page.controllers {
                for a in c.actions.values() {
                    if a.uuid == "com.elgato.streamdeck.profile.openchild"
                        && a.setting_str("ProfileUUID").eq_ignore_ascii_case(&target)
                    {
                        return Some(*id);
                    }
                }
            }
        }
        None
    }

    pub fn resolve_image(&self, page: &Uuid, rel: &str) -> Option<PathBuf> {
        if rel.is_empty() || rel.starts_with("data:") {
            return None;
        }
        let p = Path::new(rel);
        if p.is_absolute() {
            return Some(p.to_path_buf());
        }
        let candidate = self.page_dir(page).join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
        let candidate = self.dir.join(rel);
        candidate.exists().then_some(candidate)
    }

    pub fn save_manifest(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        write_json_atomic(&self.dir.join("manifest.json"), &serde_json::to_value(&self.manifest)?)
    }

    pub fn save_page(&self, page: &Uuid) -> Result<()> {
        let data = self.pages.get(page).ok_or_else(|| anyhow!("no page {page}"))?;
        let dir = self.page_dir(page);
        fs::create_dir_all(dir.join("Images"))?;
        write_json_atomic(&dir.join("manifest.json"), &serde_json::to_value(data)?)
    }

    pub fn save_all(&self) -> Result<()> {
        self.save_manifest()?;
        for id in self.pages.keys() {
            self.save_page(id)?;
        }
        Ok(())
    }

    /// Copies an image into the page's Images directory and returns the
    /// relative path to store in a state.
    pub fn store_image(&self, page: &Uuid, bytes: &[u8], ext: &str) -> Result<String> {
        let dir = self.page_dir(page).join("Images");
        fs::create_dir_all(&dir)?;
        let name = format!("{}.{}", page_dir_name(&Uuid::new_v4()), ext);
        fs::write(dir.join(&name), bytes)?;
        Ok(format!("Images/{name}"))
    }

    pub fn summary(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "name": self.manifest.name,
            "model": self.manifest.device.model,
            "device": self.manifest.device.uuid,
            "app": self.manifest.app_identifier,
            "pages": self.manifest.pages.pages,
            "currentPage": self.current_page().map(|p| p.to_string()),
            "defaultPage": self.manifest.pages.default,
        })
    }
}

pub fn write_json_atomic(path: &Path, value: &Value) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub struct ProfileStore {
    pub root: PathBuf,
    pub profiles: BTreeMap<Uuid, Profile>,
}

impl ProfileStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let mut store = ProfileStore { root, profiles: BTreeMap::new() };
        store.reload()?;
        Ok(store)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.profiles.clear();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sdProfile") {
                continue;
            }
            match load_profile(&path) {
                Ok(p) => {
                    self.profiles.insert(p.id, p);
                }
                Err(e) => tracing::warn!("skipping profile {}: {e:#}", path.display()),
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &Uuid) -> Option<&Profile> {
        self.profiles.get(id)
    }

    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut Profile> {
        self.profiles.get_mut(id)
    }

    pub fn for_device<'a>(&'a self, device_uuid: &'a str, spec: &'a DeviceSpec) -> impl Iterator<Item = &'a Profile> + 'a {
        self.profiles.values().filter(move |p| {
            p.manifest.device.uuid == device_uuid
                || (p.manifest.device.uuid.is_empty() && p.spec().map(|s| catalog::layout_compatible(s, spec)).unwrap_or(false))
        })
    }

    pub fn create(&mut self, name: &str, spec: &DeviceSpec, device_uuid: &str) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let page = Uuid::new_v4();
        let mut pages = HashMap::new();
        pages.insert(page, empty_page(spec));
        let profile = Profile {
            id,
            dir: self.root.join(format!("{}.sdProfile", id.to_string().to_uppercase())),
            manifest: ProfileManifest {
                device: DeviceRef { model: spec.model_code.into(), uuid: device_uuid.into(), ..Default::default() },
                name: name.into(),
                pages: PagesRef { current: page.to_string(), default: page.to_string(), pages: vec![page.to_string()], ..Default::default() },
                version: "2.0".into(),
                ..Default::default()
            },
            pages,
        };
        profile.save_all()?;
        self.profiles.insert(id, profile);
        Ok(id)
    }

    pub fn delete(&mut self, id: &Uuid) -> Result<()> {
        let p = self.profiles.remove(id).ok_or_else(|| anyhow!("unknown profile"))?;
        fs::remove_dir_all(&p.dir).ok();
        Ok(())
    }

    pub fn duplicate(&mut self, id: &Uuid, name: Option<&str>) -> Result<Uuid> {
        let src = self.profiles.get(id).ok_or_else(|| anyhow!("unknown profile"))?.clone();
        let new_id = Uuid::new_v4();
        let dir = self.root.join(format!("{}.sdProfile", new_id.to_string().to_uppercase()));
        copy_dir(&src.dir, &dir)?;
        let mut p = load_profile(&dir)?;
        p.id = new_id;
        p.manifest.name = name.map(String::from).unwrap_or_else(|| format!("{} copy", src.manifest.name));
        p.manifest.app_identifier = None;
        p.save_manifest()?;
        self.profiles.insert(new_id, p);
        Ok(new_id)
    }

    pub fn add_page(&mut self, id: &Uuid, spec: &DeviceSpec, folder: bool) -> Result<Uuid> {
        let p = self.profiles.get_mut(id).ok_or_else(|| anyhow!("unknown profile"))?;
        let page = Uuid::new_v4();
        let mut data = empty_page(spec);
        if folder {
            // Folders always offer a way back; the original app pins it top left.
            data.controller_mut("Keypad").actions.insert("0,0".into(), new_action("com.elgato.streamdeck.profile.backtoparent", "Parent Folder", json!({})));
        } else {
            p.manifest.pages.pages.push(page.to_string());
        }
        p.pages.insert(page, data);
        p.save_page(&page)?;
        p.save_manifest()?;
        Ok(page)
    }

    pub fn remove_page(&mut self, id: &Uuid, page: &Uuid) -> Result<()> {
        let p = self.profiles.get_mut(id).ok_or_else(|| anyhow!("unknown profile"))?;
        let key = page.to_string();
        if p.manifest.pages.pages.len() <= 1 && p.manifest.pages.pages.contains(&key) {
            bail!("a profile needs at least one page");
        }
        p.manifest.pages.pages.retain(|x| !x.eq_ignore_ascii_case(&key));
        if p.manifest.pages.current.eq_ignore_ascii_case(&key) {
            p.manifest.pages.current = p.manifest.pages.pages.first().cloned().unwrap_or_default();
        }
        if p.manifest.pages.default.eq_ignore_ascii_case(&key) {
            p.manifest.pages.default = p.manifest.pages.pages.first().cloned().unwrap_or_default();
        }
        let mut doomed = vec![*page];
        // Folders reachable only from the removed page go with it.
        let mut i = 0;
        while i < doomed.len() {
            if let Some(data) = p.pages.get(&doomed[i]) {
                for c in &data.controllers {
                    for a in c.actions.values() {
                        if a.uuid == "com.elgato.streamdeck.profile.openchild" {
                            if let Ok(child) = a.setting_str("ProfileUUID").parse::<Uuid>() {
                                if !doomed.contains(&child) {
                                    doomed.push(child);
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        for d in doomed {
            fs::remove_dir_all(p.page_dir(&d)).ok();
            p.pages.remove(&d);
        }
        p.save_manifest()
    }

    pub fn export(&self, id: &Uuid, out: &Path) -> Result<()> {
        let p = self.profiles.get(id).ok_or_else(|| anyhow!("unknown profile"))?;
        let file = fs::File::create(out)?;
        let mut zip = zip::ZipWriter::new(file);
        let root_name = p.dir.file_name().unwrap().to_string_lossy().to_string();
        add_dir_to_zip(&mut zip, &p.dir, &root_name)?;
        zip.finish()?;
        Ok(())
    }

    /// Imports a `.streamDeckProfile` archive (or an unpacked `.sdProfile`
    /// directory). Every imported profile gets a fresh UUID so importing the
    /// same file twice yields two profiles, as in the original app.
    pub fn import(&mut self, src: &Path, target: Option<(&DeviceSpec, &str)>) -> Result<Vec<Uuid>> {
        let staging = tempdir_in(&self.root)?;
        if src.is_dir() {
            copy_dir(src, &staging.join(src.file_name().unwrap()))?;
        } else {
            unzip(src, &staging)?;
        }
        let mut found = Vec::new();
        find_sdprofiles(&staging, &mut found);
        // A v1 profile nests its folders as Profiles/<id>.sdProfile; only the
        // outermost directories are profiles.
        found.retain(|p| !p.ancestors().skip(1).any(|a| a.extension().and_then(|e| e.to_str()) == Some("sdProfile")));
        let mut ids = Vec::new();
        for dir in found {
            let manifest: Value = serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)?;
            let new_id = Uuid::new_v4();
            let dest = self.root.join(format!("{}.sdProfile", new_id.to_string().to_uppercase()));
            let is_v1 = manifest.get("Actions").is_some() || manifest.get("Version").and_then(|v| v.as_str()).map(|v| v.starts_with('1')).unwrap_or(false);
            if is_v1 {
                convert_v1(&dir, &dest)?;
            } else {
                copy_dir(&dir, &dest)?;
            }
            let mut p = load_profile(&dest)?;
            p.id = new_id;
            if let Some((spec, device_uuid)) = target {
                let compatible = p.spec().map(|s| catalog::layout_compatible(s, spec)).unwrap_or(true);
                if compatible {
                    p.manifest.device.uuid = device_uuid.to_string();
                    if p.manifest.device.model.is_empty() {
                        p.manifest.device.model = spec.model_code.into();
                    }
                }
            }
            p.save_manifest()?;
            ids.push(new_id);
            self.profiles.insert(new_id, p);
        }
        fs::remove_dir_all(&staging).ok();
        if ids.is_empty() {
            bail!("no profiles found in {}", src.display());
        }
        Ok(ids)
    }

    pub fn backup(&self, out: &Path) -> Result<()> {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(out)?;
        let mut zip = zip::ZipWriter::new(file);
        add_dir_to_zip(&mut zip, &self.root, "ProfilesV2")?;
        let settings = crate::paths::settings_file();
        if settings.exists() {
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("settings.json", opts)?;
            zip.write_all(&fs::read(settings)?)?;
        }
        zip.finish()?;
        Ok(())
    }

    /// Replaces every profile with the backup's. The current set is backed
    /// up first so a restore can itself be undone.
    pub fn restore(&mut self, backup: &Path) -> Result<()> {
        let safety = crate::paths::backups_dir().join(format!("before-restore-{}.streamDeckProfilesBackup", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        self.backup(&safety)?;
        let staging = tempdir_in(&self.root.parent().unwrap_or(&self.root).to_path_buf())?;
        unzip(backup, &staging)?;
        let src = if staging.join("ProfilesV2").is_dir() { staging.join("ProfilesV2") } else { staging.clone() };
        let mut found = Vec::new();
        find_sdprofiles(&src, &mut found);
        if found.is_empty() {
            fs::remove_dir_all(&staging).ok();
            bail!("backup contains no profiles");
        }
        for p in self.profiles.values() {
            fs::remove_dir_all(&p.dir).ok();
        }
        for dir in found.iter().filter(|p| p.parent() == Some(src.as_path())) {
            copy_dir(dir, &self.root.join(dir.file_name().unwrap()))?;
        }
        fs::remove_dir_all(&staging).ok();
        self.reload()
    }
}

pub fn empty_page(spec: &DeviceSpec) -> PageData {
    let mut page = PageData::default();
    if spec.encoders > 0 {
        page.controllers.push(Controller { kind: "Encoder".into(), ..Default::default() });
    }
    page.controllers.push(Controller { kind: "Keypad".into(), ..Default::default() });
    page
}

pub fn new_action(uuid: &str, name: &str, settings: Value) -> ActionData {
    ActionData {
        action_id: Uuid::new_v4().to_string(),
        uuid: uuid.into(),
        name: name.into(),
        settings,
        state: 0,
        states: vec![StateData::default()],
        linked_title: true,
        ..Default::default()
    }
}

pub fn load_profile(dir: &Path) -> Result<Profile> {
    let stem = dir.file_stem().and_then(|s| s.to_str()).ok_or_else(|| anyhow!("bad dir"))?;
    let id: Uuid = stem.parse().with_context(|| format!("profile dir {stem} is not a UUID"))?;
    let manifest: ProfileManifest = serde_json::from_slice(&fs::read(dir.join("manifest.json"))?).context("profile manifest")?;
    let mut pages = HashMap::new();
    if let Ok(rd) = fs::read_dir(dir.join("Profiles")) {
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(page_id) = page_id_from_dir(&name) else { continue };
            match fs::read(path.join("manifest.json")).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<PageData>(&b)?)) {
                Ok(mut page) => {
                    ensure_action_ids(&mut page);
                    pages.insert(page_id, page);
                }
                Err(e) => tracing::warn!("page {}: {e:#}", path.display()),
            }
        }
    }
    let mut profile = Profile { id, dir: dir.to_path_buf(), manifest, pages };
    // Pages listed in the manifest but missing on disk would leave the deck blank.
    let spec = profile.spec().unwrap_or(&catalog::SPECS[1]);
    let listed: Vec<Uuid> = profile.top_pages();
    for page in listed {
        profile.pages.entry(page).or_insert_with(|| empty_page(spec));
    }
    if profile.manifest.pages.pages.is_empty() {
        let page = Uuid::new_v4();
        profile.pages.insert(page, empty_page(spec));
        profile.manifest.pages.pages.push(page.to_string());
        profile.manifest.pages.current = page.to_string();
    }
    Ok(profile)
}

/// Every action needs a stable ActionID: it is the SDK context plugins see.
fn ensure_action_ids(page: &mut PageData) {
    fn fix(a: &mut ActionData) {
        if a.action_id.is_empty() {
            a.action_id = Uuid::new_v4().to_string();
        }
        if let Some(routines) = a.actions.as_mut() {
            for r in routines {
                for child in &mut r.actions {
                    fix(child);
                }
            }
        }
    }
    for c in &mut page.controllers {
        for a in c.actions.values_mut() {
            fix(a);
        }
    }
}

/// Profiles from Stream Deck 4.x/5.x: actions sit directly in the profile
/// manifest and folders are nested `Profiles/<id>.sdProfile` directories.
fn convert_v1(src: &Path, dest: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_slice(&fs::read(src.join("manifest.json"))?)?;
    let model = manifest.get("DeviceModel").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let device_uuid = manifest.get("DeviceUUID").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = manifest.get("Name").and_then(|v| v.as_str()).unwrap_or("Imported").to_string();
    let root_page = Uuid::new_v4();
    let profile_tmp = Profile {
        id: Uuid::new_v4(),
        dir: dest.to_path_buf(),
        manifest: ProfileManifest {
            device: DeviceRef { model, uuid: device_uuid, ..Default::default() },
            name,
            pages: PagesRef { current: root_page.to_string(), default: root_page.to_string(), pages: vec![root_page.to_string()], ..Default::default() },
            version: "2.0".into(),
            app_identifier: manifest.get("AppIdentifier").and_then(|v| v.as_str()).map(String::from),
            ..Default::default()
        },
        pages: HashMap::new(),
    };
    let mut profile = profile_tmp;
    fn walk(profile: &mut Profile, src: &Path, page_id: Uuid) -> Result<()> {
        let m: Value = serde_json::from_slice(&fs::read(src.join("manifest.json"))?)?;
        let mut page = PageData::default();
        let keypad = page.controller_mut("Keypad");
        if let Some(actions) = m.get("Actions").and_then(|a| a.as_object()) {
            for (pos, a) in actions {
                let mut action: ActionData = serde_json::from_value(a.clone()).unwrap_or_default();
                if action.action_id.is_empty() {
                    action.action_id = Uuid::new_v4().to_string();
                }
                keypad.actions.insert(pos.clone(), action);
            }
        }
        let page_dir = profile.page_dir(&page_id);
        fs::create_dir_all(page_dir.join("Images"))?;
        // v1 stored images as "<pos>/CustomImages/state0.png"; copy them into Images/.
        for (pos, action) in keypad.actions.iter_mut() {
            for (i, st) in action.states.iter_mut().enumerate() {
                let legacy = src.join(pos).join("CustomImages").join(format!("state{i}.png"));
                if legacy.exists() {
                    let file = format!("{}.png", page_dir_name(&Uuid::new_v4()));
                    fs::copy(&legacy, page_dir.join("Images").join(&file))?;
                    st.image = Some(format!("Images/{file}"));
                }
            }
        }
        // Folders become sibling pages referenced by their openchild actions.
        let mut children = Vec::new();
        for action in keypad.actions.values_mut() {
            if action.uuid == "com.elgato.streamdeck.profile.openchild" {
                let child = action.setting_str("ProfileUUID");
                let child_dir = src.join("Profiles").join(format!("{}.sdProfile", child.to_uppercase()));
                let child_dir = if child_dir.exists() { child_dir } else { src.join("Profiles").join(format!("{child}.sdProfile")) };
                if child_dir.exists() {
                    let new_child = Uuid::new_v4();
                    action.settings = json!({ "ProfileUUID": new_child.to_string() });
                    children.push((child_dir, new_child));
                }
            }
        }
        profile.pages.insert(page_id, page);
        for (dir, id) in children {
            walk(profile, &dir, id)?;
        }
        Ok(())
    }
    walk(&mut profile, src, root_page)?;
    profile.save_all()
}

fn find_sdprofiles(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.extension().and_then(|e| e.to_str()) == Some("sdProfile") && p.join("manifest.json").exists() {
                    out.push(p.clone());
                }
                find_sdprofiles(&p, out);
            }
        }
    }
}

pub fn tempdir_in(base: &Path) -> Result<PathBuf> {
    let dir = base.join(format!(".tmp-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// Extracts a zip, refusing entries that would escape `dest`.
pub fn unzip(src: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(src).with_context(|| format!("open {}", src.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("not a zip archive")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue };
        if rel.components().any(|c| c.as_os_str() == "__MACOSX") {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            fs::write(&out, buf)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o755 | 0o600)).ok();
            }
        }
    }
    Ok(())
}

fn add_dir_to_zip<W: Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, dir: &Path, prefix: &str) -> Result<()> {
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory(format!("{prefix}/"), opts)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(".tmp-") || name.ends_with(".tmp") {
            continue;
        }
        let path = entry.path();
        let zname = format!("{prefix}/{name}");
        if path.is_dir() {
            add_dir_to_zip(zip, &path, &zname)?;
        } else {
            zip.start_file(zname, opts)?;
            zip.write_all(&fs::read(&path)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_page_and_reload() {
        let root = std::env::temp_dir().join(format!("sdstore-{}", Uuid::new_v4()));
        let mut store = ProfileStore::open(root.clone()).unwrap();
        let spec = catalog::spec("plus").unwrap();
        let id = store.create("Test", spec, "dev").unwrap();
        let folder = store.add_page(&id, spec, true).unwrap();
        store.add_page(&id, spec, false).unwrap();
        let mut again = ProfileStore::open(root.clone()).unwrap();
        let p = again.get(&id).unwrap();
        assert_eq!(p.top_pages().len(), 2);
        assert!(p.is_folder(&folder));
        assert_eq!(p.pages[&folder].controller("Keypad").unwrap().actions["0,0"].uuid, "com.elgato.streamdeck.profile.backtoparent");
        let out = root.join("x.streamDeckProfile");
        again.export(&id, &out).unwrap();
        let ids = again.import(&out, None).unwrap();
        assert_eq!(again.get(&ids[0]).unwrap().pages.len(), 3);
        fs::remove_dir_all(root).ok();
    }
}
