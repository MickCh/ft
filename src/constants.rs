#[cfg(windows)]
pub const NEW_LINE: &str = "\r\n";
#[cfg(not(windows))]
pub const NEW_LINE: &str = "\n";
