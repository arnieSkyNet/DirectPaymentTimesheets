//! Shared ComboBox interaction: scrollbar gestures keep the popup open,
//! outside clicks dismiss it, and selecting an item closes it (even if unchanged).
pub fn combo_box(id_salt: impl std::hash::Hash) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id_salt)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

pub fn combo_value<T: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    selected: T,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.selectable_value(current, selected, text);
    if response.clicked() {
        ui.close();
    }
    response
}
