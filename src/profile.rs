use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const TEST_FLAG: &str = "--test-profile";
pub const INACTIVE_FLAG: &str = "--inactive";
pub const PORTABLE_MARKER: &str = "portable.txt";
const PORTABLE_DATA: &str = "data";

#[derive(Clone, Debug, Default, PartialEq)]
enum Place {
    #[default]
    Standard,
    Portable(PathBuf),
    Test(PathBuf),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    place: Place,
}

static CURRENT: OnceLock<Profile> = OnceLock::new();

pub fn init(profile: Profile) {
    if let Place::Test(root) = &profile.place {
        let _ = std::fs::create_dir_all(root.join("Temp"));
    }
    let _ = CURRENT.set(profile);
}

pub fn current() -> &'static Profile {
    CURRENT.get_or_init(Profile::default)
}

fn trimmed(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    let kept = text.trim_end_matches(['\\', '/']);
    if kept.is_empty() || kept.ends_with(':') {
        return path;
    }
    PathBuf::from(kept)
}

impl Profile {
    pub fn test(root: PathBuf) -> Self {
        Self { place: Place::Test(trimmed(root)) }
    }

    pub fn portable_beside(exe: &Path) -> Option<Self> {
        let folder = exe.parent()?;
        folder.join(PORTABLE_MARKER).is_file().then(|| Self { place: Place::Portable(folder.to_path_buf()) })
    }

    pub fn from_args(args: impl IntoIterator<Item = OsString>, stop: &str) -> Result<Self, String> {
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == stop {
                break;
            }
            if arg == TEST_FLAG {
                return match args.next().filter(|root| !root.is_empty() && *root != stop) {
                    Some(root) if Path::new(&root).is_absolute() => Ok(Self::test(root.into())),
                    _ => Err(format!("{TEST_FLAG} needs an absolute folder")),
                };
            }
        }
        Ok(Self::default())
    }

    pub fn testing(&self) -> bool {
        matches!(self.place, Place::Test(_))
    }

    pub fn portable(&self) -> bool {
        matches!(self.place, Place::Portable(_))
    }

    pub fn local_data(&self) -> PathBuf {
        match &self.place {
            Place::Test(root) | Place::Portable(root) => root.clone(),
            Place::Standard => PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into())),
        }
    }

    pub fn data_root(&self) -> PathBuf {
        match &self.place {
            Place::Portable(folder) => folder.join(PORTABLE_DATA),
            Place::Standard | Place::Test(_) => self.local_data().join("Traydex"),
        }
    }

    pub fn temp_dir(&self) -> PathBuf {
        match &self.place {
            Place::Test(root) => root.join("Temp"),
            Place::Standard | Place::Portable(_) => std::env::temp_dir(),
        }
    }

    pub fn name(&self) -> &'static str {
        if self.testing() { "Traydex Test" } else { "Traydex" }
    }

    pub fn mutex(&self) -> &'static str {
        if self.testing() { "Local\\Traydex.Test.Singleton" } else { "Local\\Traydex.Singleton" }
    }

    pub fn relay_class(&self) -> &'static str {
        if self.testing() { "Traydex.Test.Relay" } else { "Traydex.Relay" }
    }

    pub fn registry_name(&self) -> &'static str {
        if self.testing() { "TraydexTest" } else { "Traydex" }
    }

    pub fn autostart_name(&self) -> &'static str {
        self.name()
    }

    pub fn command(&self, exe: &str, rest: &str) -> String {
        match &self.place {
            Place::Test(root) => format!("\"{exe}\" {TEST_FLAG} \"{}\" {rest}", root.display()),
            Place::Standard | Place::Portable(_) => format!("\"{exe}\" {rest}"),
        }
    }
}

#[cfg(test)]
mod tests;
