use bevy::prelude::*;
use bevy_yarn_slinger::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use yarn_slinger::prelude::{CompilationType, YarnCompiler};

#[test]
fn loads_yarn_assets() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_plugin(YarnSlingerPlugin::with_localizations(None));

    let asset_server = app.world.get_resource::<AssetServer>().unwrap();
    let handle = asset_server.load("lines.yarn");

    app.update();

    let yarn_file_assets = app.world.get_resource::<Assets<YarnFile>>().unwrap();
    let yarn_file = yarn_file_assets.get(&handle).unwrap();

    let expected_source = include_str!("../assets/lines.yarn");
    assert_eq!(expected_source, yarn_file.file.source);
    assert_eq!("lines.yarn", yarn_file.file.file_name);
}

#[test]
fn generates_localization_files() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let original_yarn_path = project_root_path().join("assets/lines.yarn");
    let yarn_path = dir.path().join("lines.yarn");
    fs::copy(&original_yarn_path, &yarn_path)?;

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(AssetPlugin {
        asset_folder: dir.path().to_str().unwrap().to_string(),
        ..default()
    }))
    .add_plugin(YarnSlingerPlugin::with_localizations(Localizations {
        base_language: "en-US".into(),
        translations: vec!["de-CH".into()],
        file_generation_mode: FileGenerationMode::Development,
    }));
    let asset_server = app.world.get_resource_mut::<AssetServer>().unwrap();
    let handle = asset_server.load("lines.yarn");

    app.update(); // read yarn
    app.update(); // write line IDs
    app.update(); // write strings files
    app.update(); // rewrite strings files?

    let yarn_file_assets = app.world.get_resource::<Assets<YarnFile>>().unwrap();
    let yarn_file_in_app = yarn_file_assets.get(&handle).unwrap();
    let yarn_file_on_disk = fs::read_to_string(&yarn_path)?;
    assert_eq!(yarn_file_in_app.file.source, yarn_file_on_disk);
    let string_table_without_line_ids = YarnCompiler::new()
        .read_file(&original_yarn_path)
        .with_compilation_type(CompilationType::StringsOnly)
        .compile()?
        .string_table;
    let string_table_with_line_ids = YarnCompiler::new()
        .read_file(&yarn_path)
        .with_compilation_type(CompilationType::StringsOnly)
        .compile()?
        .string_table;

    assert!(string_table_with_line_ids
        .values()
        .all(|string_info| !string_info.is_implicit_tag));
    assert_eq!(
        string_table_without_line_ids.len(),
        string_table_with_line_ids.len()
    );

    assert!(!dir.path().join("en-US.strings.csv").exists());
    let translation_strings_table_path = dir.path().join("de-CH.strings.csv");
    assert!(translation_strings_table_path.exists());
    let translation_strings_table = fs::read_to_string(&translation_strings_table_path)?;
    let line_ids_in_strings_table: Vec<_> = translation_strings_table
        .lines()
        .skip(1)
        .map(|line| line.split(',').nth(1).unwrap())
        .collect();

    println!("{:#?}", line_ids_in_strings_table);
    assert_eq!(
        string_table_with_line_ids.len(),
        line_ids_in_strings_table.len()
    );

    assert!(line_ids_in_strings_table
        .iter()
        .all(|line_id| string_table_with_line_ids.contains_key(&LineId(line_id.to_string()))));

    Ok(())
}
pub fn project_root_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
