//! Copy text to the Windows clipboard.

pub(crate) fn copy_text(text: &str) -> Result<(), String> {
    use windows::ApplicationModel::DataTransfer::{Clipboard, DataPackage};
    (|| {
        let data = DataPackage::new()?;
        data.SetText(&windows::core::HSTRING::from(text))?;
        Clipboard::SetContent(&data)
    })()
    .map_err(|e| format!("Could not copy to the clipboard: {e}"))
}
