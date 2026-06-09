use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use snb_core::plugin::PluginInfo;

const SNB_MANIFEST: &str = "snb.toml";
const CARGO_MANIFEST: &str = "Cargo.toml";

#[derive(Clone, Debug)]
pub(crate) struct PluginIndex {
    pub(crate) root: PathBuf,
    pub(crate) plugins_root: PathBuf,
    pub(crate) locals: Vec<LocalPlugin>,
    pub(crate) loaded: Vec<PluginInfo>,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalPlugin {
    pub(crate) dir_name: String,
    pub(crate) dir: PathBuf,
    pub(crate) package_name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) library_name: Option<String>,
    pub(crate) declared_names: Vec<String>,
    pub(crate) has_snb_manifest: bool,
}

#[derive(Clone, Debug)]
struct CargoInfo {
    package_name: String,
    version: Option<String>,
    library_name: String,
}

#[derive(Clone, Debug, Default)]
struct SnbInfo {
    source: Option<PathBuf>,
    manifest: Option<PathBuf>,
    package_name: Option<String>,
    version: Option<String>,
}

impl PluginIndex {
    pub(crate) fn discover() -> anyhow::Result<Self> {
        let root = runtime_root()?;
        let plugins_root = root.join("plugins");
        let locals = discover_local_plugins(&plugins_root)?;
        let loaded = loaded_plugins();
        Ok(Self {
            root,
            plugins_root,
            locals,
            loaded,
        })
    }

    pub(crate) fn resolve_local(&self, query: &str) -> anyhow::Result<LocalPlugin> {
        let query_path = resolve_path(&self.root, query);
        let manifest_parent = query_path
            .file_name()
            .is_some_and(|name| name == CARGO_MANIFEST || name == SNB_MANIFEST)
            .then(|| query_path.parent().map(Path::to_path_buf))
            .flatten();

        let matches = self
            .locals
            .iter()
            .filter(|plugin| {
                plugin.matches_query(query)
                    || plugin.path_matches(&query_path)
                    || manifest_parent
                        .as_ref()
                        .is_some_and(|path| plugin.path_matches(path))
            })
            .cloned()
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [plugin] => Ok(plugin.clone()),
            [] => anyhow::bail!(
                "local plugin not found: {query}. Known plugins: {}",
                self.local_names()
            ),
            _ => anyhow::bail!(
                "plugin name '{query}' is ambiguous: {}",
                matches
                    .iter()
                    .map(LocalPlugin::display_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub(crate) fn resolve_loaded_name(&self, query: &str) -> anyhow::Result<String> {
        let mut matches = self
            .loaded
            .iter()
            .filter(|info| identifier_matches(&info.name, query))
            .map(|info| info.name.clone())
            .collect::<Vec<_>>();

        for local in self
            .locals
            .iter()
            .filter(|local| local.matches_query(query))
        {
            if let Some(info) = self.loaded_for_local(local) {
                matches.push(info.name);
            }
        }

        dedup(&mut matches);
        match matches.as_slice() {
            [name] => Ok(name.clone()),
            [] => anyhow::bail!(
                "loaded plugin not found: {query}. Loaded plugins: {}",
                self.loaded_names()
            ),
            _ => anyhow::bail!(
                "loaded plugin name '{query}' is ambiguous: {}",
                matches.join(", ")
            ),
        }
    }

    pub(crate) fn loaded_for_local(&self, local: &LocalPlugin) -> Option<PluginInfo> {
        self.loaded
            .iter()
            .find(|info| local.matches_query(&info.name))
            .cloned()
    }

    pub(crate) fn local_for_loaded(&self, loaded_name: &str) -> Option<LocalPlugin> {
        self.locals
            .iter()
            .find(|local| local.matches_query(loaded_name))
            .cloned()
    }

    pub(crate) fn loaded_without_local(&self) -> Vec<PluginInfo> {
        self.loaded
            .iter()
            .filter(|info| self.local_for_loaded(&info.name).is_none())
            .cloned()
            .collect()
    }

    pub(crate) fn resolve_load_target(&self, input: &str) -> anyhow::Result<PathBuf> {
        let path = resolve_path(&self.root, input);
        if is_dynamic_library_path(&path) || path.is_file() {
            if !path.is_file() {
                anyhow::bail!("plugin library not found: {}", path.display());
            }
            return Ok(path);
        }

        let plugin = self.resolve_local(input)?;
        plugin.existing_library_path(&self.root).ok_or_else(|| {
            anyhow::anyhow!(
                "no built library found for {}. Expected {}. Run /plugin update {} first.",
                plugin.display_name(),
                plugin.release_library_path(&self.root).display(),
                plugin.dir_name
            )
        })
    }

    fn local_names(&self) -> String {
        if self.locals.is_empty() {
            return "-".to_string();
        }
        self.locals
            .iter()
            .map(LocalPlugin::display_name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn loaded_names(&self) -> String {
        if self.loaded.is_empty() {
            return "-".to_string();
        }
        self.loaded
            .iter()
            .map(|info| info.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl LocalPlugin {
    fn from_dir(dir: &Path) -> anyhow::Result<Option<Self>> {
        let snb_manifest = dir.join(SNB_MANIFEST);
        let cargo_manifest = dir.join(CARGO_MANIFEST);
        if !snb_manifest.is_file() && !cargo_manifest.is_file() {
            return Ok(None);
        }

        let cargo = if cargo_manifest.is_file() {
            Some(read_cargo_info(&cargo_manifest)?)
        } else {
            None
        };
        let snb = if snb_manifest.is_file() {
            Some(read_snb_info(dir, &snb_manifest)?)
        } else {
            None
        };

        let snb_manifest_cargo = snb
            .as_ref()
            .and_then(|info| info.manifest.as_ref())
            .and_then(|manifest| read_cargo_info(manifest).ok());

        let package_name = snb
            .as_ref()
            .and_then(|info| info.package_name.clone())
            .or_else(|| cargo.as_ref().map(|info| info.package_name.clone()));
        let version = snb
            .as_ref()
            .and_then(|info| info.version.clone())
            .or_else(|| cargo.as_ref().and_then(|info| info.version.clone()));
        let library_name = snb
            .as_ref()
            .and_then(|info| info.package_name.clone())
            .or_else(|| {
                snb_manifest_cargo
                    .as_ref()
                    .map(|info| info.library_name.clone())
            })
            .or_else(|| cargo.as_ref().map(|info| info.library_name.clone()));

        let mut source_files = Vec::new();
        if let Some(snb) = &snb {
            if let Some(source) = &snb.source {
                source_files.push(source.clone());
            }
            if let Some(manifest) = &snb.manifest
                && let Some(parent) = manifest.parent()
            {
                collect_rs_files(&parent.join("src"), &mut source_files);
            }
        }
        collect_rs_files(&dir.join("src"), &mut source_files);
        source_files.sort();
        source_files.dedup();

        let mut declared_names = Vec::new();
        for source in source_files {
            if let Ok(content) = fs::read_to_string(&source) {
                declared_names.extend(extract_declared_names(&content));
            }
        }
        dedup(&mut declared_names);

        let dir_name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>")
            .to_string();

        Ok(Some(Self {
            dir_name,
            dir: dir.to_path_buf(),
            package_name,
            version,
            library_name,
            declared_names,
            has_snb_manifest: snb_manifest.is_file(),
        }))
    }

    pub(crate) fn display_name(&self) -> String {
        self.declared_names
            .first()
            .cloned()
            .or_else(|| self.package_name.clone())
            .unwrap_or_else(|| self.dir_name.clone())
    }

    pub(crate) fn identifiers(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.push(self.dir_name.clone());
        names.extend(short_names(&self.dir_name));
        if let Some(package_name) = &self.package_name {
            names.push(package_name.clone());
            names.extend(short_names(package_name));
        }
        if let Some(library_name) = &self.library_name {
            names.push(library_name.clone());
            names.extend(short_names(library_name));
        }
        for name in &self.declared_names {
            names.push(name.clone());
            names.extend(short_names(name));
        }
        dedup(&mut names);
        names
    }

    pub(crate) fn matches_query(&self, query: &str) -> bool {
        self.identifiers()
            .iter()
            .any(|identifier| identifier_matches(identifier, query))
    }

    pub(crate) fn path_matches(&self, path: &Path) -> bool {
        normalize_path(&self.dir) == normalize_path(path)
    }

    pub(crate) fn release_library_path(&self, root: &Path) -> PathBuf {
        root.join("target")
            .join("release")
            .join(dynamic_library_file_name(
                self.library_name.as_deref().unwrap_or(&self.dir_name),
            ))
    }

    pub(crate) fn existing_library_path(&self, root: &Path) -> Option<PathBuf> {
        self.library_paths(root)
            .into_iter()
            .find(|path| path.is_file())
    }

    pub(crate) fn library_paths(&self, root: &Path) -> Vec<PathBuf> {
        let Some(library_name) = self.library_name.as_deref() else {
            return Vec::new();
        };
        let file_name = dynamic_library_file_name(library_name);
        vec![
            root.join("target").join("release").join(&file_name),
            self.dir.join("target").join("release").join(&file_name),
            root.join("target").join("debug").join(&file_name),
            self.dir.join("target").join("debug").join(&file_name),
        ]
    }

    pub(crate) fn relative_dir(&self, root: &Path) -> String {
        self.dir
            .strip_prefix(root)
            .unwrap_or(&self.dir)
            .to_string_lossy()
            .into_owned()
    }

    pub(crate) fn manifest_kind(&self) -> &'static str {
        if self.has_snb_manifest {
            "snb"
        } else {
            "cargo"
        }
    }
}

pub(crate) fn validate_plugin_dir_name(name: &str) -> anyhow::Result<()> {
    let path = Path::new(name);
    let mut components = path.components();
    let Some(Component::Normal(component)) = components.next() else {
        anyhow::bail!("plugin dir must be a directory name under plugins/");
    };
    if components.next().is_some() {
        anyhow::bail!("plugin dir must not contain path separators");
    }
    if component.to_string_lossy().starts_with('.') {
        anyhow::bail!("plugin dir must not start with '.'");
    }
    Ok(())
}

pub(crate) fn dynamic_library_file_name(crate_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{crate_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{crate_name}.dylib")
    } else {
        format!("lib{crate_name}.so")
    }
}

pub(crate) fn resolve_path(root: &Path, input: &str) -> PathBuf {
    let path = PathBuf::from(input);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

fn runtime_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    for candidate in cwd.ancestors() {
        if candidate.join("plugins").is_dir() {
            return Ok(candidate.to_path_buf());
        }
    }
    Ok(cwd)
}

fn discover_local_plugins(plugins_root: &Path) -> anyhow::Result<Vec<LocalPlugin>> {
    let Ok(entries) = fs::read_dir(plugins_root) else {
        return Ok(Vec::new());
    };

    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match LocalPlugin::from_dir(&path) {
            Ok(Some(plugin)) => plugins.push(plugin),
            Ok(None) => {}
            Err(error) => log::warn!("skip plugin directory {}: {error:#}", path.display()),
        }
    }
    plugins.sort_by_key(LocalPlugin::display_name);
    Ok(plugins)
}

fn loaded_plugins() -> Vec<PluginInfo> {
    let bot = snb_core::context::bot();
    let mut plugins = bot
        .list_plugins()
        .into_iter()
        .filter_map(|name| bot.get_plugin(&name))
        .collect::<Vec<_>>();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn read_cargo_info(manifest: &Path) -> anyhow::Result<CargoInfo> {
    let content = fs::read_to_string(manifest)?;
    let table = content.parse::<toml::Table>()?;
    let package = table
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow::anyhow!("{} must contain [package]", manifest.display()))?;
    let package_name = package
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{} [package].name is required", manifest.display()))?
        .to_string();
    let version = package
        .get("version")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);
    let library_name = table
        .get("lib")
        .and_then(toml::Value::as_table)
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
        .map(sanitize_crate_name)
        .unwrap_or_else(|| sanitize_crate_name(&package_name));

    Ok(CargoInfo {
        package_name,
        version,
        library_name,
    })
}

fn read_snb_info(plugin_dir: &Path, manifest: &Path) -> anyhow::Result<SnbInfo> {
    let content = fs::read_to_string(manifest)?;
    let table = content.parse::<toml::Table>()?;
    let build = table.get("build").and_then(toml::Value::as_table);
    let Some(build) = build else {
        return Ok(SnbInfo::default());
    };

    let source = build
        .get("source")
        .and_then(toml::Value::as_str)
        .map(|value| plugin_dir.join(value));
    let manifest = build
        .get("manifest")
        .and_then(toml::Value::as_str)
        .map(|value| plugin_dir.join(value));
    let package_name = build
        .get("package")
        .or_else(|| build.get("name"))
        .and_then(toml::Value::as_str)
        .map(sanitize_crate_name);
    let version = build
        .get("version")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);

    Ok(SnbInfo {
        source,
        manifest,
        package_name,
        version,
    })
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn extract_declared_names(content: &str) -> Vec<String> {
    let consts = extract_string_consts(content);
    let mut names = Vec::new();
    names.extend(extract_plugin_attribute_names(content));
    names.extend(extract_name_method_names(content, &consts));
    names.extend(
        consts
            .iter()
            .filter(|(name, _)| name.contains("PLUGIN_NAME"))
            .map(|(_, value)| value.clone()),
    );
    dedup(&mut names);
    names
}

fn extract_plugin_attribute_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = content;
    while let Some(pos) = rest.find("#[plugin") {
        let attr = &rest[pos..];
        let end = attr.find(']').unwrap_or(attr.len());
        let attr = &attr[..end];
        if let Some(name) = extract_keyed_string(attr, "name") {
            names.push(name);
        }
        rest = &attr[end.min(attr.len())..];
        if rest.is_empty() {
            break;
        }
    }
    names
}

fn extract_name_method_names(content: &str, consts: &HashMap<String, String>) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = content;
    while let Some(pos) = rest.find("fn name") {
        let after = &rest[pos..];
        let body_start = after.find('{').unwrap_or(0);
        let body = &after[body_start..after.len().min(body_start + 300)];
        if let Some(name) = extract_first_string(body) {
            names.push(name);
        } else {
            for (const_name, value) in consts {
                if body.contains(const_name) {
                    names.push(value.clone());
                }
            }
        }
        rest = &after[body.len()..];
        if rest.is_empty() {
            break;
        }
    }
    names
}

fn extract_string_consts(content: &str) -> HashMap<String, String> {
    let mut consts = HashMap::new();
    for line in content.lines().map(str::trim) {
        let Some(after_const) = line
            .strip_prefix("const ")
            .or_else(|| line.strip_prefix("pub const "))
        else {
            continue;
        };
        let name_end = after_const
            .find(|ch: char| ch == ':' || ch == '=' || ch.is_whitespace())
            .unwrap_or(after_const.len());
        let const_name = after_const[..name_end].trim();
        if const_name.is_empty() {
            continue;
        }
        if let Some(value) = extract_first_string(after_const) {
            consts.insert(const_name.to_string(), value);
        }
    }
    consts
}

fn extract_keyed_string(text: &str, key: &str) -> Option<String> {
    let pos = text.find(key)?;
    let after_key = &text[pos + key.len()..];
    let after_equals = after_key.strip_prefix(after_key.split('=').next()?)?;
    extract_first_string(after_equals)
}

fn extract_first_string(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in text[start + 1..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn identifier_matches(candidate: &str, requested: &str) -> bool {
    if candidate.eq_ignore_ascii_case(requested) {
        return true;
    }

    let candidate_norm = normalize_name(candidate);
    let requested_norm = normalize_name(requested);
    if candidate_norm == requested_norm {
        return true;
    }

    let candidate_loose = loose_key(candidate);
    let requested_loose = loose_key(requested);
    if candidate_loose == requested_loose {
        return true;
    }

    short_names(candidate).iter().any(|short| {
        let short_norm = normalize_name(short);
        short_norm == requested_norm || loose_key(short) == requested_loose
    })
}

fn short_names(name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let normalized = normalize_name(name);
    let prefixes = ["snb_adapter_", "snb_database_", "snb_plugin_", "snb_"];
    for prefix in prefixes {
        if let Some(short) = normalized.strip_prefix(prefix) {
            names.push(short.to_string());
        }
    }
    for suffix in ["_rs", "-rs"] {
        if let Some(short) = normalized.strip_suffix(suffix) {
            names.push(short.to_string());
            names.extend(short_names(short));
        }
    }
    for suffix in ["adapter", "plugin", "bot"] {
        let loose = loose_key(name);
        if let Some(short) = loose.strip_suffix(suffix)
            && !short.is_empty()
        {
            names.push(short.to_string());
        }
    }
    names
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch == '-' { '_' } else { ch })
        .collect()
}

fn loose_key(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sanitize_crate_name(name: &str) -> String {
    name.replace('-', "_")
}

fn is_dynamic_library_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "dll" | "so" | "dylib"))
}

fn dedup(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(normalize_name(value)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_library_name_uses_platform_convention() {
        let name = dynamic_library_file_name("snb_plugin_manager");
        if cfg!(target_os = "windows") {
            assert_eq!(name, "snb_plugin_manager.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, "libsnb_plugin_manager.dylib");
        } else {
            assert_eq!(name, "libsnb_plugin_manager.so");
        }
    }

    #[test]
    fn cargo_library_name_reads_package_name() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert_eq!(
            read_cargo_info(&manifest).unwrap().library_name,
            "snb_plugin_manager"
        );
    }

    #[test]
    fn plugin_attribute_name_is_extracted() {
        let content = r#"#[plugin(name = "plugin_manager", version = "0.1.0", kind = Plugin)]"#;
        assert_eq!(
            extract_plugin_attribute_names(content),
            vec!["plugin_manager"]
        );
    }

    #[test]
    fn const_backed_name_method_is_extracted() {
        let content = r#"
            const PLUGIN_NAME: &str = "PayloadExtractBot";
            impl SnbPlugin for PayloadExtractBot {
                fn name(&self) -> &str { PLUGIN_NAME }
            }
        "#;
        assert_eq!(extract_declared_names(content), vec!["PayloadExtractBot"]);
    }

    #[test]
    fn fuzzy_matching_handles_prefixes_suffixes_and_case() {
        assert!(identifier_matches("snb_adapter_tg", "tg"));
        assert!(identifier_matches(
            "PayloadExtractBot",
            "payload_extract_bot"
        ));
        assert!(identifier_matches(
            "payload_extract_bot-rs",
            "payload_extract_bot"
        ));
    }

    #[test]
    fn local_discovery_detects_known_runtime_names_when_sources_exist() {
        let root = runtime_root().unwrap();
        let plugins = discover_local_plugins(&root.join("plugins")).unwrap();

        if let Some(tg) = plugins
            .iter()
            .find(|plugin| plugin.dir_name == "snb_adapter_tg")
        {
            assert!(tg.matches_query("TGAdapter"));
            assert!(tg.matches_query("tg"));
        }

        if let Some(payload) = plugins
            .iter()
            .find(|plugin| plugin.dir_name == "payload_extract_bot-rs")
        {
            assert!(payload.matches_query("PayloadExtractBot"));
            assert_eq!(
                payload.library_name.as_deref(),
                Some("snb_payload_extract_bot")
            );
        }
    }

    #[test]
    fn resolve_local_accepts_displayed_dir_and_manifest_paths() {
        let root = runtime_root().unwrap();
        if !root.join("plugins").join("snb_adapter_tg").is_dir() {
            return;
        }

        let index = PluginIndex {
            plugins_root: root.join("plugins"),
            locals: discover_local_plugins(&root.join("plugins")).unwrap(),
            root,
            loaded: Vec::new(),
        };

        assert_eq!(
            index
                .resolve_local("plugins/snb_adapter_tg")
                .unwrap()
                .dir_name,
            "snb_adapter_tg"
        );
        assert_eq!(
            index
                .resolve_local("plugins/snb_adapter_tg/Cargo.toml")
                .unwrap()
                .dir_name,
            "snb_adapter_tg"
        );
    }
}
