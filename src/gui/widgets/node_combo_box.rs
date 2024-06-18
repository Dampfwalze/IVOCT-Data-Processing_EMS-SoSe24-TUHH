#![allow(dead_code)]

use egui::{
    emath::TSTransform, style::WidgetVisuals, AboveOrBelow, ComboBox, InnerResponse, Rect,
    Response, Ui, WidgetText,
};

pub struct NodeComboBox(ComboBox);

impl NodeComboBox {
    /// Create new [`ComboBox`] with id and label
    pub fn new(id_source: impl std::hash::Hash, label: impl Into<WidgetText>) -> Self {
        Self(ComboBox::new(id_source, label))
    }

    /// Label shown next to the combo box
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        Self(ComboBox::from_label(label))
    }

    /// Without label.
    pub fn from_id_source(id_source: impl std::hash::Hash) -> Self {
        Self(ComboBox::from_id_source(id_source))
    }

    /// Set the outer width of the button and menu.
    ///
    /// Default is [`Spacing::combo_width`].
    #[inline]
    pub fn width(mut self, width: f32) -> Self {
        self.0 = self.0.width(width);
        self
    }

    /// Set the maximum outer height of the menu.
    ///
    /// Default is [`Spacing::combo_height`].
    #[inline]
    pub fn height(mut self, height: f32) -> Self {
        self.0 = self.0.height(height);
        self
    }

    /// What we show as the currently selected value
    #[inline]
    pub fn selected_text(mut self, selected_text: impl Into<WidgetText>) -> Self {
        self.0 = self.0.selected_text(selected_text);
        self
    }

    /// Use the provided function to render a different [`ComboBox`] icon.
    /// Defaults to a triangle that expands when the cursor is hovering over the [`ComboBox`].
    ///
    /// For example, see [ComboBox::icon].
    pub fn icon(
        mut self,
        icon_fn: impl FnOnce(&Ui, Rect, &WidgetVisuals, bool, AboveOrBelow) + 'static,
    ) -> Self {
        self.0 = self.0.icon(icon_fn);
        self
    }

    /// Controls whether text wrap is used for the selected text
    #[inline]
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.0 = self.0.wrap(wrap);
        self
    }

    /// Show the combo box, with the given ui code for the menu contents.
    ///
    /// Returns `InnerResponse { inner: None }` if the combo box is closed.
    pub fn show_ui<R>(
        self,
        ui: &mut Ui,
        menu_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let transform = get_transform(ui);

        self.0.show_ui(ui, |ui| {
            ui.ctx().set_transform_layer(ui.layer_id(), transform);
            menu_contents(ui)
        })
    }

    /// Show a list of items with the given selected index.
    ///
    /// For example, see [ComboBox::show_index].
    pub fn show_index<Text: Into<WidgetText>>(
        self,
        ui: &mut Ui,
        selected: &mut usize,
        len: usize,
        get: impl Fn(usize) -> Text,
    ) -> Response {
        let transform = get_transform(ui);

        let slf = self.selected_text(get(*selected));

        let mut changed = false;

        let mut response = slf
            .show_ui(ui, |ui| {
                ui.ctx().set_transform_layer(ui.layer_id(), transform);
                for i in 0..len {
                    if ui.selectable_label(i == *selected, get(i)).clicked() {
                        *selected = i;
                        changed = true;
                    }
                }
            })
            .response;

        if changed {
            response.mark_changed();
        }
        response
    }
}

fn get_transform(ui: &mut Ui) -> TSTransform {
    ui.ctx()
        .memory_mut(|mem| mem.layer_transforms.get(&ui.layer_id()).copied())
        .unwrap_or(TSTransform::IDENTITY)
}
