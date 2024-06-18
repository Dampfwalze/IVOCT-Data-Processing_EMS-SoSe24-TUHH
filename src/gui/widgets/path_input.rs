use std::{borrow::Cow, mem, path::PathBuf};

use egui::{
    text::{CCursor, CCursorRange},
    text_selection::text_cursor_state::byte_index_from_char_index,
    vec2, Response, TextBuffer, TextEdit, Widget,
};
use native_dialog::FileDialog;

pub struct PathInput<'a> {
    path: &'a mut PathBuf,
}

impl<'a> PathInput<'a> {
    pub fn new(path: &'a mut PathBuf) -> Self {
        Self { path }
    }
}

impl<'a> Widget for PathInput<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        ui.horizontal(|ui| {
            ui.allocate_ui(vec2(ui.available_width() - 25.0, 18.0), |ui| {
                let mut output = TextEdit::singleline(&mut PathWrapper(self.path))
                    .hint_text("Path")
                    .show(ui);

                if output.response.gained_focus() {
                    output.state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(0),
                        CCursor::new(self.path.as_os_str().len()),
                    )));

                    output.state.store(ui.ctx(), output.response.id);
                }
            });

            if ui.button("...").clicked() {
                match FileDialog::new().show_open_single_file() {
                    Ok(Some(path)) => {
                        *self.path = path;
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("Error getting file from dialog: {}", e),
                }
            }
        })
        .response
    }
}

#[repr(transparent)]
struct PathWrapper<'a>(&'a mut PathBuf);

impl<'a> TextBuffer for PathWrapper<'a> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        let cow = self.0.to_string_lossy();
        match cow {
            Cow::Borrowed(s) => s,
            Cow::Owned(_) => unreachable!(),
        }
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let byte_idx = byte_index_from_char_index(self.as_str(), char_index);

        // Take ownership of the path, by replacing it with an empty path.
        let this = mem::replace(self.0, PathBuf::new());

        // Convert the path into a string
        let mut str = this.into_os_string().into_string().unwrap();

        // Insert the text into the string
        str.insert_str(byte_idx, text);

        // Write the string back into the path
        *self.0 = PathBuf::from(str);

        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
        assert!(char_range.start <= char_range.end);

        let byte_start = byte_index_from_char_index(self.as_str(), char_range.start);
        let byte_end = byte_index_from_char_index(self.as_str(), char_range.end);

        let this = mem::replace(self.0, PathBuf::new());
        let mut str = this.into_os_string().into_string().unwrap();

        str.drain(byte_start..byte_end);

        *self.0 = PathBuf::from(str);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn replace_with(&mut self, text: &str) {
        *self.0 = PathBuf::from(text);
    }

    fn take(&mut self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
