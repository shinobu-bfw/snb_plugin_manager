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
    let content = r#"#[plugin(name = "PluginManager", version = "0.1.1", kind = Plugin)]"#;
    assert_eq!(
        extract_plugin_attribute_names(content),
        vec!["PluginManager"]
    );
}

#[test]
fn const_backed_name_method_is_extracted() {
    let content = r#"
        const PLUGIN_NAME: &str = "PayloadExtract";
        impl SnbPlugin for PayloadExtract {
            fn name(&self) -> &str { PLUGIN_NAME }
        }
    "#;
    assert_eq!(extract_declared_names(content), vec!["PayloadExtract"]);
}

#[test]
fn fuzzy_matching_handles_prefixes_suffixes_and_case() {
    assert!(identifier_matches("snb_adapter_tg", "tg"));
    assert!(identifier_matches("PayloadExtract", "payload_extract"));
    assert!(identifier_matches("payload_extract-rs", "payload_extract"));
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
