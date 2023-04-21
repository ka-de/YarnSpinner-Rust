//! Adapted from <https://github.com/YarnSpinnerTool/YarnSpinner/blob/da39c7195107d8211f21c263e4084f773b84eaff/YarnSpinner.Compiler/StringTableManager.cs>

use crate::output::StringInfo;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Default)]
pub(crate) struct StringTableManager(pub HashMap<String, StringInfo>);

impl StringTableManager {
    pub(crate) fn contains_implicit_string_tags(&self) -> bool {
        self.values().any(|x| x.is_implicit_tag)
    }

    pub(crate) fn insert(&mut self, line_id: impl Into<Option<String>>, string_info: StringInfo) {
        let line_id = line_id.into();
        let (line_id, string_info) = if let Some(line_id) = line_id {
            let string_info = StringInfo {
                is_implicit_tag: false,
                ..string_info
            };
            (line_id, string_info)
        } else {
            let line_id = format!(
                "line:{}-{}-{}",
                string_info.file_name,
                string_info.node_name,
                self.len()
            );
            let string_info = StringInfo {
                is_implicit_tag: true,
                ..string_info
            };
            (line_id, string_info)
        };
        self.0.insert(line_id, string_info);
    }
}

impl Deref for StringTableManager {
    type Target = HashMap<String, StringInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StringTableManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
