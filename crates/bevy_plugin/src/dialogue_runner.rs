pub use self::events::{
    DialogueCompleteEvent, DialogueStartEvent, ExecuteCommandEvent, LineHintsEvent,
    NodeCompleteEvent, NodeStartEvent, PresentLineEvent, PresentOptionsEvent,
};
pub use self::{
    builder::{DialogueRunnerBuilder, StartNode},
    dialogue_option::DialogueOption,
    inner::{InnerDialogue, InnerDialogueMut},
    localized_line::LocalizedLine,
};
use crate::line_provider::LineAssets;
use crate::prelude::*;
use crate::UnderlyingYarnLine;
use anyhow::bail;
use bevy::utils::HashSet;
use bevy::{prelude::*, tasks::Task, utils::HashMap};
pub(crate) use runtime_interaction::DialogueExecutionSystemSet;
use std::any::TypeId;
use std::fmt::Debug;
use yarn_slinger::core::Library;

mod builder;
mod dialogue_option;
mod events;
mod inner;
mod localized_line;
mod runtime_interaction;

pub(crate) fn dialogue_plugin(app: &mut App) {
    app.fn_plugin(runtime_interaction::runtime_interaction_plugin)
        .fn_plugin(localized_line::localized_line_plugin)
        .fn_plugin(events::dialogue_runner_events_plugin)
        .fn_plugin(dialogue_option::dialogue_option_plugin)
        .fn_plugin(builder::dialogue_runner_builder_plugin)
        .fn_plugin(inner::inner_dialogue_runner_plugin);
}

#[derive(Debug, Component)]
pub struct DialogueRunner {
    pub(crate) dialogue: Dialogue,
    pub(crate) text_provider: Box<dyn TextProvider>,
    asset_providers: HashMap<TypeId, Box<dyn AssetProvider>>,
    pub will_continue_in_next_update: bool,
    pub(crate) last_selected_option: Option<OptionId>,
    pub(crate) commands: YarnCommandRegistrations,
    command_tasks: Vec<Task<()>>,
    localizations: Option<Localizations>,
    pub(crate) is_running: bool,
    pub run_selected_options_as_lines: bool,
    pub(crate) just_started: bool,
    pub(crate) popped_line_hints: Option<Vec<LineId>>,
}

impl DialogueRunner {
    pub const DEFAULT_START_NODE_NAME: &'static str = Dialogue::DEFAULT_START_NODE_NAME;

    pub fn continue_in_next_update(&mut self) -> &mut Self {
        self.try_continue_in_next_update()
            .unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn try_continue_in_next_update(&mut self) -> Result<&mut Self> {
        if !self.is_running {
            bail!("Can't continue dialogue that isn't running. Please call `DialogueRunner::start()` or `DialogueRunner::start_at_node(..)` before calling `DialogueRunner::continue_in_next_update()`.");
        }
        self.will_continue_in_next_update = true;
        Ok(self)
    }

    pub fn select_option(&mut self, option: OptionId) -> Result<&mut Self> {
        if !self.is_running {
            bail!("Can't select option {option}: the dialogue is currently not running. Please call `DialogueRunner::continue_in_next_update()` only after receiving a `PresentOptionsEvent`.")
        }
        self.last_selected_option.replace(option);
        self.dialogue
            .set_selected_option(option)
            .map_err(Error::from)?;
        self.continue_in_next_update();
        Ok(self)
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn stop(&mut self) -> &mut Self {
        self.is_running = false;
        self.dialogue.stop();
        self
    }

    pub fn clear(&mut self) -> &mut Self {
        self.is_running = false;
        self.dialogue.unload_all();
        self
    }

    pub fn start(&mut self) -> Result<&mut Self> {
        if self.is_running {
            bail!("Can't start dialogue: the dialogue is currently in the middle of running. Stop the dialogue first.");
        }
        self.is_running = true;
        self.just_started = true;
        self.continue_in_next_update();
        Ok(self)
    }

    pub fn start_at_node(&mut self, node_name: impl AsRef<str>) -> Result<&mut Self> {
        let node_name = node_name.as_ref();
        if self.is_running {
            bail!("Can't start dialogue from node {node_name}: the dialogue is currently in the middle of running. Stop the dialogue first.");
        }
        self.is_running = true;
        self.just_started = true;
        self.dialogue
            .set_node(node_name)
            .with_context(|| format!("Can't start dialogue from node {node_name}:"))?;
        self.continue_in_next_update();
        Ok(self)
    }

    #[must_use]
    pub fn get_tags_for_node(&self, node_name: &str) -> Option<Vec<String>> {
        self.dialogue.get_tags_for_node(node_name)
    }

    #[must_use]
    pub fn node_exists(&self, node_name: &str) -> bool {
        self.dialogue.node_exists(node_name)
    }

    #[must_use]
    pub fn current_node(&self) -> Option<String> {
        self.dialogue.current_node()
    }

    #[must_use]
    pub fn variable_storage(&self) -> &dyn VariableStorage {
        self.dialogue.variable_storage()
    }

    #[must_use]
    pub fn variable_storage_mut(&mut self) -> &mut dyn VariableStorage {
        self.dialogue.variable_storage_mut()
    }

    #[must_use]
    pub fn are_lines_available(&self) -> bool {
        self.are_texts_available() && self.are_assets_available()
    }

    #[must_use]
    fn are_texts_available(&self) -> bool {
        self.text_provider.are_lines_available()
    }

    #[must_use]
    fn are_assets_available(&self) -> bool {
        self.asset_providers
            .values()
            .all(|provider| provider.are_assets_available())
    }

    pub fn set_language(&mut self, language: impl Into<Language>) -> &mut Self {
        let language = language.into();
        self.set_text_language(language.clone())
            .set_asset_language(language)
    }

    pub fn set_text_language(&mut self, language: impl Into<Language>) -> &mut Self {
        let language = language.into();
        self.assert_localizations_available_for_language(&language);
        self.dialogue.set_language_code(language);
        self
    }

    pub fn set_asset_language(&mut self, language: impl Into<Language>) -> &mut Self {
        let language = language.into();
        self.assert_localizations_available_for_language(&language);
        for asset_provider in self.asset_providers.values_mut() {
            asset_provider.set_language(language.clone().into());
        }
        self
    }

    fn assert_localizations_available_for_language(&self, language: &Language) {
        let localizations = self.localizations.as_ref().expect(
            "Tried to set language, but no localizations are available. \
            Did you forget to call `YarnSlingerApp::with_localizations(..)` on the plugin setup?",
        );
        assert!(
            localizations.supports_language(language),
            "Tried to set language to {language}, but no localizations are available for that language."
        );
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        self.dialogue.library()
    }

    #[must_use]
    pub fn library_mut(&mut self) -> &mut Library {
        self.dialogue.library_mut()
    }

    #[must_use]
    pub fn command_registrations(&self) -> &YarnCommandRegistrations {
        &self.commands
    }

    #[must_use]
    pub fn command_registrations_mut(&mut self) -> &mut YarnCommandRegistrations {
        &mut self.commands
    }

    #[must_use]
    pub fn text_language(&self) -> Option<Language> {
        self.dialogue.language_code().cloned()
    }

    #[must_use]
    pub fn asset_language(&self) -> Option<Language> {
        let languages: HashSet<_> = self
            .asset_providers
            .values()
            .map(|provider| provider.get_language())
            .collect();
        assert!(
            languages.len() <= 1,
            "Asset providers have different languages"
        );
        languages.into_iter().next().flatten()
    }

    #[must_use]
    pub fn inner(&self) -> InnerDialogue {
        InnerDialogue(&self.dialogue)
    }

    #[must_use]
    pub fn inner_mut(&mut self) -> InnerDialogueMut {
        InnerDialogueMut(&mut self.dialogue)
    }

    #[must_use]
    pub fn text_provider(&self) -> &dyn TextProvider {
        self.text_provider.as_ref()
    }

    #[must_use]
    pub fn asset_provider<T: 'static>(&self) -> Option<&dyn AssetProvider> {
        self.asset_providers
            .get(&TypeId::of::<T>())
            .map(|p| p.as_ref())
    }

    pub fn asset_providers(&self) -> impl Iterator<Item = &dyn AssetProvider> {
        self.asset_providers.values().map(|p| p.as_ref())
    }

    #[must_use]
    pub(crate) fn get_assets(&self, line_id: &UnderlyingYarnLine) -> LineAssets {
        self.asset_providers
            .values()
            .map(|p| p.get_assets(line_id))
            .collect()
    }

    pub(crate) fn add_command_task(&mut self, task: Task<()>) -> &mut Self {
        self.command_tasks.push(task);
        self
    }

    #[must_use]
    pub(crate) fn poll_tasks_and_check_if_done(&mut self) -> bool {
        self.command_tasks.retain(|task| !task.is_finished());
        self.command_tasks.is_empty()
    }
}
