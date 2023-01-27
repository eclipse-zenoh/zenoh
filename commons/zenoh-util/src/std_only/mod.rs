pub mod ffi;
mod lib_loader;
pub mod net;
pub mod time_range;
pub use lib_loader::*;
pub mod timer;
pub use timer::*;
/// The "ZENOH_HOME" environement variable name
pub const ZENOH_HOME_ENV_VAR: &str = "ZENOH_HOME";

const DEFAULT_ZENOH_HOME_DIRNAME: &str = ".zenoh";

/// Return the path to the ${ZENOH_HOME} directory (~/.zenoh by default).
pub fn zenoh_home() -> &'static std::path::Path {
    use std::path::PathBuf;
    lazy_static! {
        static ref ROOT: PathBuf = {
            if let Some(dir) = std::env::var_os(ZENOH_HOME_ENV_VAR) {
                PathBuf::from(dir)
            } else {
                match home::home_dir() {
                    Some(mut dir) => {
                        dir.push(DEFAULT_ZENOH_HOME_DIRNAME);
                        dir
                    }
                    None => PathBuf::from(DEFAULT_ZENOH_HOME_DIRNAME),
                }
            }
        };
    }
    ROOT.as_path()
}
