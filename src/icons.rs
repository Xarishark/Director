//! Shared SVG action icons, staged beside the executable by the build.
use windows_reactor::*;
fn svg(name: &str, size: f64) -> View {
    let mut directory = std::env::current_exe().unwrap_or_default();
    directory.pop();
    if directory.file_name().is_some_and(|n| n == "deps") {
        directory.pop();
    }
    Image::new()
        .source_file(directory.join("icons").join(name))
        .map(|image| image.width(size).height(size).into())
        .unwrap_or_else(|_| View::empty())
}
/// The remote-control icon, sized for card buttons.
pub(crate) fn remote() -> View {
    svg("remote.svg", 18.0)
}
pub(crate) fn delete() -> View {
    svg("delete.svg", 20.0)
}
/// The delete icon at the 16 px size used inside card buttons.
pub(crate) fn delete_small() -> View {
    svg("delete.svg", 16.0)
}
