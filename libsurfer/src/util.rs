//! Utility functions.
use crate::{displayed_item_tree::VisibleItemIndex, wave_data::WaveData};
use camino::Utf8PathBuf;
use egui::RichText;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

/// This function takes a number and converts it's digits into the range
/// a-p. This is nice because it makes for some easily typed ids.
/// The function first formats the number as a hex digit and then performs
/// the mapping.
#[must_use]
pub(crate) fn uint_idx_to_alpha_idx(idx: VisibleItemIndex, nvariables: usize) -> String {
    // this calculates how many hex digits we need to represent nvariables
    // unwrap because the result should always fit into usize and because
    // we are not going to display millions of character ids.
    let width = usize::try_from(nvariables.ilog(16)).unwrap() + 1;
    format!("{:0width$x}", idx.0)
        .chars()
        .map(|c| match c {
            '0' => 'a',
            '1' => 'b',
            '2' => 'c',
            '3' => 'd',
            '4' => 'e',
            '5' => 'f',
            '6' => 'g',
            '7' => 'h',
            '8' => 'i',
            '9' => 'j',
            'a' => 'k',
            'b' => 'l',
            'c' => 'm',
            'd' => 'n',
            'e' => 'o',
            'f' => 'p',
            _ => '?',
        })
        .collect()
}

/// This is the reverse function to `uint_idx_to_alpha_idx`.
pub(crate) fn alpha_idx_to_uint_idx(idx: &str) -> Option<VisibleItemIndex> {
    let mapped = idx
        .chars()
        .map(|c| match c {
            'a' => '0',
            'b' => '1',
            'c' => '2',
            'd' => '3',
            'e' => '4',
            'f' => '5',
            'g' => '6',
            'h' => '7',
            'i' => '8',
            'j' => '9',
            'k' => 'a',
            'l' => 'b',
            'm' => 'c',
            'n' => 'd',
            'o' => 'e',
            'p' => 'f',
            _ => '?',
        })
        .collect::<String>();
    usize::from_str_radix(&mapped, 16)
        .ok()
        .map(VisibleItemIndex)
}

#[must_use]
pub(crate) fn get_alpha_focus_id(vidx: VisibleItemIndex, waves: &WaveData) -> RichText {
    let alpha_id = uint_idx_to_alpha_idx(vidx, waves.displayed_items.len());

    RichText::new(alpha_id).monospace()
}

/// This function searches upward from `start` for directories or files matching `item`. It returns
/// a `Vec<PathBuf>` to all found instances in order of closest to furthest away. The function only
/// searches up within subdirectories of `end`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn search_upward(
    start: impl AsRef<Path>,
    end: impl AsRef<Path>,
    item: impl AsRef<Path>,
) -> Vec<PathBuf> {
    start
        .as_ref()
        .ancestors()
        .take_while(|p| p.starts_with(end.as_ref()))
        .map(|p| p.join(&item))
        .filter(|p| p.try_exists().is_ok_and(std::convert::identity))
        .collect()
}

fn get_multi_extension_from_filename(filename: &str) -> Option<String> {
    filename
        .find('.')
        .map(|pos| filename[pos + 1..].to_string())
}

/// Get the full extension of a path, including all extensions.
/// For example, for "foo.tar.gz", this function returns "tar.gz", and not just "gz",
/// like `path.extension()` would.
#[must_use]
pub(crate) fn get_multi_extension(path: &Utf8PathBuf) -> Option<String> {
    // Find the first . in the path, if any. Return the rest of the path.
    if let Some(filename) = path.file_name() {
        return get_multi_extension_from_filename(filename);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uint_idx_to_alpha_idx_basic_width_1() {
        // nvariables determines hex width: width = ilog16(nvariables) + 1
        // For nvariables = 1 => width = 1
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0), 1), "a");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(9), 1), "j");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(15), 1), "p");
    }

    #[test]
    fn test_uint_idx_to_alpha_idx_zero_padded_width_2() {
        // nvariables = 16 => width = 2 (since ilog16(16) == 1)
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0x0), 16), "aa");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0x1), 16), "ab");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0xf), 16), "ap");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0x10), 16), "ba");
        assert_eq!(uint_idx_to_alpha_idx(VisibleItemIndex(0x1f), 16), "bp");
    }

    #[test]
    fn test_alpha_idx_to_uint_idx_roundtrip() {
        // Try a selection across multiple widths
        let cases = [
            (VisibleItemIndex(0x0), 1),
            (VisibleItemIndex(0x9), 1),
            (VisibleItemIndex(0xf), 1),
            (VisibleItemIndex(0x10), 16),
            (VisibleItemIndex(0x2a), 256),
            (VisibleItemIndex(0xabc), 4096),
        ];

        for (vidx, nvars) in cases {
            let s = uint_idx_to_alpha_idx(vidx, nvars);
            let back = alpha_idx_to_uint_idx(&s).expect("should parse back");
            assert_eq!(back, vidx);
        }
    }

    #[test]
    fn test_alpha_idx_to_uint_idx_invalid_input() {
        // Contains invalid character 'r' which is outside a-p
        assert!(alpha_idx_to_uint_idx("ar").is_none());
        // Empty string should fail to parse as hex
        assert!(alpha_idx_to_uint_idx("").is_none());
        // Mixed case / unexpected chars
        assert!(alpha_idx_to_uint_idx("A").is_none());
        assert!(alpha_idx_to_uint_idx("-").is_none());
    }

    #[test]
    fn test_get_multi_extension_from_filename() {
        assert_eq!(
            get_multi_extension_from_filename("foo.tar.gz"),
            Some("tar.gz".to_string())
        );
        assert_eq!(
            get_multi_extension_from_filename("foo.txt"),
            Some("txt".to_string())
        );
        assert_eq!(get_multi_extension_from_filename("foo"), None);
        // Leading dot files: first dot at 0, extension is the remainder
        assert_eq!(
            get_multi_extension_from_filename(".bashrc"),
            Some("bashrc".to_string())
        );
        // Trailing dot: extension becomes empty string
        assert_eq!(
            get_multi_extension_from_filename("foo."),
            Some(String::new())
        );
    }

    #[test]
    fn test_get_multi_extension_from_path() {
        let p = Utf8PathBuf::from("/tmp/foo/bar.tar.gz");
        assert_eq!(get_multi_extension(&p), Some("tar.gz".to_string()));
        let p = Utf8PathBuf::from("/tmp/foo/bar");
        assert_eq!(get_multi_extension(&p), None);
    }

    #[test]
    fn test_get_multi_extension_with_unicode() {
        // Ensure Unicode before the first dot does not break slicing
        // (previous implementation mixed byte and char indexing)
        let name = "åäö.archive.tar.gz"; // multibyte chars before '.'
        assert_eq!(
            get_multi_extension_from_filename(name),
            Some("archive.tar.gz".to_string())
        );

        // Only Unicode and then dot
        let name2 = "ß.";
        assert_eq!(
            get_multi_extension_from_filename(name2),
            Some(String::new())
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_search_upward_finds_closest_first() {
        use std::fs;
        use std::io::Write;
        use std::path::Path;

        // Create a temporary directory structure: root/a/b/c
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let a = root.join("a");
        let b = a.join("b");
        let c = b.join("c");
        fs::create_dir_all(&c).expect("dirs");

        // Place target file at c and at a
        let item_name = Path::new("target.txt");
        let item_c = c.join(item_name);
        let item_a = a.join(item_name);
        {
            let mut f = fs::File::create(&item_c).expect("create c");
            writeln!(f, "hello").unwrap();
        }
        {
            let mut f = fs::File::create(&item_a).expect("create a");
            writeln!(f, "world").unwrap();
        }

        // Start searching from c upwards, but only within root
        let found = search_upward(&c, root, item_name);
        // Expect closest-first order: c/target.txt, then a/target.txt
        assert_eq!(found, vec![item_c, item_a]);
    }
}
