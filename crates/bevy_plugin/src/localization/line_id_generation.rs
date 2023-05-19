use crate::prelude::*;
use bevy::prelude::*;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, SystemSet)]
pub(crate) struct LineIdUpdateSystemSet;

pub(crate) fn line_id_generation_plugin(app: &mut App) {
    app.add_system(
        generate_missing_line_ids_in_yarn_file
            .pipe(panic_on_err)
            .in_set(LineIdUpdateSystemSet)
            .run_if(is_in_development),
    );
}

fn generate_missing_line_ids_in_yarn_file(
    mut events: EventReader<AssetEvent<YarnFile>>,
    mut assets: ResMut<Assets<YarnFile>>,
    asset_server: Res<AssetServer>,
) -> SystemResult {
    for event in events.iter() {
        let (AssetEvent::Created { handle } | AssetEvent::Modified { handle }) = event else {
            continue;
        };
        let yarn_file = assets.get(handle).unwrap().clone();
        let Some(source_with_added_ids) = add_tags_to_lines(yarn_file)? else {
            continue;
        };
        let yarn_file = assets.get_mut(handle).unwrap();

        let asset_path = asset_server
            .get_handle_path(handle.clone())
            .with_context(|| format!("Failed to overwrite Yarn file \"{}\" with new IDs because it was not found on disk",
                                     yarn_file.file_name()))?;
        let assets_path = get_assets_dir_path(&asset_server)?;
        let path_within_asset_dir: PathBuf =
            [assets_path.as_ref(), asset_path.path()].iter().collect();

        std::fs::write(&path_within_asset_dir, &source_with_added_ids)
                    .context(
                        format!("Failed to overwrite Yarn file at {} with new line IDs. \
                                 Aborting because localization requires all lines to have IDs, but this file is missing some.",
                                path_within_asset_dir.display()))?;

        yarn_file.file.source = source_with_added_ids;

        let string_table = YarnCompiler::new()
            .with_compilation_type(CompilationType::StringsOnly)
            .add_file(yarn_file.file.clone())
            .compile()?
            .string_table;
        yarn_file.string_table = string_table;
    }
    Ok(())
}

/// Adapted from <https://github.com/YarnSpinnerTool/YarnSpinner-Console/blob/main/src/YarnSpinner.Console/Commands/TagCommand.cs#L11>
fn add_tags_to_lines(yarn_file: YarnFile) -> YarnCompilerResult<Option<String>> {
    let existing_tags = yarn_file
        .string_table
        .into_iter()
        .filter_map(|(key, string_info)| (!string_info.is_implicit_tag).then(|| key.clone()))
        .collect();
    YarnCompiler::add_tags_to_lines(yarn_file.file.source, existing_tags)
}
