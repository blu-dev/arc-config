#![feature(let_else)]
#![feature(let_chains)]
use std::{collections::HashMap, path::Path};

use camino::Utf8Path;
use hash40::Hash40;
use serde::{Deserialize, Serialize};

pub mod generate;

pub mod search;

/// The base ARCropolis mod configuation format.
///
/// This format enables the user to have some control over how the filesystem is recreated,
/// in order to save size on their mod distributions, or to just modify the filesystem itself,
/// with oversight.
#[derive(Serialize, Deserialize)]
pub struct Config {
    /// The unsharing blacklist prevents a file from being unshared automatically.
    ///
    /// This enables mod creators to distribute character redesign mods without worrying about
    /// needing to replace that file across all skins of the character.
    #[serde(alias = "keep-shared")]
    #[serde(alias = "keep_shared")]
    #[serde(alias = "unshare-blacklist")]
    #[serde(default = "Vec::new")]
    pub unshare_blacklist: Vec<Hash40>,

    /// The preprocess-reshare field is used internally by ARCropolis to share files
    /// inside of Dark Samus's victory screen package with her actual costume, enabling the victory
    /// screen body double to share the same skin and prevent crashes.
    #[serde(alias = "preprocess-reshare")]
    #[serde(default = "HashMap::new")]
    pub preprocess_reshare: HashMap<Hash40, Hash40>,

    /// Allows users to specify files to share to vanilla files. This is valid for files
    /// which currently do not exist in the filesystem, or files which already do.
    ///
    /// For example, the following would be valid in order to share Mario's second costume
    /// slot with his first one:
    /// ```json
    /// {
    ///     "share-to-vanilla": {
    ///         "fighter/mario/model/body/c00/def_mario_001_col.nutexb": "fighter/mario/model/body/c01/def_mario_001_col.nutexb"
    ///     }
    /// }
    /// ```
    ///
    /// This field can take signular strings (as shown above), or it can take a set of strings (or `NewFile` structures, but those are usually
    /// handled by tools which auto-generate the config).
    /// ```json
    /// {
    ///     "share-to-vanilla": {
    ///         "fighter/mario/model/body/c00/def_mario_001_col.nutexb": [
    ///             "fighter/mario/model/body/c01/def_mario_001_col.nutexb",
    ///             "fighter/mario/model/body/c02/def_mario_001_col.nutexb"    
    ///         ]
    ///     }
    /// }
    /// ```
    #[serde(alias = "share-to-vanilla")]
    #[serde(default = "HashMap::new")]
    pub share_to_vanilla: HashMap<Hash40, search::FileSet>,

    /// Allows users to specify files to share to added fiels. This is valid for
    /// fiels which currently do not exist in the filesystem, or fiels which already do.
    ///
    /// For example, the following would share Mario's first costume to a new file placed somewhere else
    /// in the filesystem:
    /// ```json
    /// {
    ///     "share-to-added": {
    ///         "fighter/mario/custom_skins/mario_slot_c00.nutexb": "fighter/mario/model/body/c00/def_mario_001_col.nutexb"
    ///     }
    /// }
    /// ```
    ///
    /// Similar to `share-to-vanilla`, this field can also take a set of entries instead of just a singular one.
    #[serde(alias = "share-to-added")]
    #[serde(alias = "new-shared-files")]
    #[serde(alias = "new_shared_files")]
    #[serde(default = "HashMap::new")]
    pub share_to_added: HashMap<Hash40, search::FileSet>,

    /// Allows users to specify which file package to add a file to. This enables the filesystem to load the file at
    /// the correct time as it would load other files
    #[serde(alias = "new-dir-files")]
    #[serde(default = "HashMap::new")]
    pub new_dir_files: HashMap<Hash40, Vec<Hash40>>,
}

impl Config {
    /// Helper method to deserialize the mod configuration from a JSON string
    pub fn from_json<S: AsRef<str>>(json: S) -> serde_json::Result<Self> {
        serde_json::from_str(json.as_ref())
    }

    /// Helper method to deserialize the mod configuration from a JSON file
    pub fn from_file_json<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        std::fs::read_to_string(path).and_then(|string| {
            serde_json::from_str(&string).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Json Deserialization Error: {:?}", e),
                )
            })
        })
    }

    /// Helper method to serialize the mod configuration to a JSON file
    pub fn to_file_json<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        serde_json::to_string_pretty(self)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Json Serialization Error: {:?}", e),
                )
            })
            .and_then(|string| std::fs::write(path, string))
    }

    /// Helper method to merge two mod configurations
    pub fn merge(&mut self, other: Self) {
        let Self {
            unshare_blacklist,
            preprocess_reshare,
            share_to_vanilla,
            share_to_added,
            new_dir_files,
        } = other;

        self.unshare_blacklist.extend(unshare_blacklist);
        self.preprocess_reshare.extend(preprocess_reshare);

        for (k, v) in share_to_vanilla {
            if let Some(set) = self.share_to_vanilla.get_mut(&k) {
                set.0.extend(v.0);
            } else {
                self.share_to_vanilla.insert(k, v);
            }
        }

        for (k, v) in share_to_added {
            if let Some(set) = self.share_to_added.get_mut(&k) {
                set.0.extend(v.0);
            } else {
                self.share_to_added.insert(k, v);
            }
        }

        self.new_dir_files.extend(new_dir_files);
    }
}

/// Convenience method for converting a path to Hash40, allowing an inter-mix of hashes and strings on a component basis.
///
/// For example, both of the following are the same:
/// ```
/// path_to_hash("fighter/mario/model/body/c00/model.numdlb");
/// path_to_hash("fighter/mario/0x5d79572d9/body/c00/model.numdlb");
/// ```
/// This method is agnostic of path separators since it uses the path's components.
pub fn path_to_hash(path: &Utf8Path) -> Hash40 {
    // start with an empty hash
    let mut hashed = Hash40::new("");
    for component in path.components() {
        // get the component as a string
        let component = component.as_str();

        hashed = if hashed == Hash40(0) {
            // get the label from the hash, which can either by a hex string or
            Hash40::from_label(component).unwrap()
        } else if component.starts_with("0x") && component.contains('.') {
            // if the component is a hex string AND it contains a period, we expect it to be in the format of
            // <file_name_hash>.<extension>, since the file name hash also includes the extension but we need the extension
            // when generating the search section
            hashed.join_path(Hash40::from_label(component.split_once('.').unwrap().0).unwrap())
        } else {
            // otherwise we just want to join the path
            hashed.join_path(Hash40::from_label(component).unwrap())
        }
    }

    hashed
}
