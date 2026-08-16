use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const MIN_WIDTH: f64 = 390.0;
pub const MIN_HEIGHT: f64 = 640.0;
pub const DEFAULT_WIDTH: f64 = 1120.0;
pub const DEFAULT_HEIGHT: f64 = 760.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedSize {
    pub width: f64,
    pub height: f64,
}

impl Default for SavedSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

/// Keeps a restored size inside the monitor work area and the supported minimum.
#[must_use]
pub fn clamp_logical(width: f64, height: f64, work_width: f64, work_height: f64) -> SavedSize {
    let max_width = work_width.max(1.0);
    let max_height = work_height.max(1.0);
    SavedSize {
        width: width.max(MIN_WIDTH.min(max_width)).min(max_width),
        height: height.max(MIN_HEIGHT.min(max_height)).min(max_height),
    }
}

#[must_use]
pub fn load(path: &Path) -> SavedSize {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the last window size. Failures are logged by the caller.
///
/// # Errors
///
/// Returns an I/O or encode error when the file cannot be replaced.
pub fn save(path: &Path, size: SavedSize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&size).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_respects_minimum_and_work_area() {
        assert_eq!(
            clamp_logical(200.0, 200.0, 1920.0, 1080.0),
            SavedSize {
                width: MIN_WIDTH,
                height: MIN_HEIGHT
            }
        );
        assert_eq!(
            clamp_logical(3000.0, 2000.0, 1024.0, 768.0),
            SavedSize {
                width: 1024.0,
                height: 768.0
            }
        );
    }

    #[test]
    fn clamp_shrinks_to_a_smaller_work_area() {
        assert_eq!(
            clamp_logical(800.0, 900.0, 360.0, 600.0),
            SavedSize {
                width: 360.0,
                height: 600.0
            }
        );
    }

    #[test]
    fn save_round_trips_the_last_size() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("window-size.json");
        let size = SavedSize {
            width: 900.0,
            height: 700.0,
        };
        save(&path, size).expect("save");
        assert_eq!(load(&path), size);
        assert_eq!(
            load(&directory.path().join("missing.json")),
            SavedSize::default()
        );
    }
}
