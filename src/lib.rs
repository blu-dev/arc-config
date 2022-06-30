#![feature(let_else)]
#![feature(let_chains)]
use std::collections::HashMap;

use hash40::Hash40;
use serde::{Deserialize, Serialize};

mod search;

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
    pub new_dir_files: HashMap<Hash40, Hash40>,
}
