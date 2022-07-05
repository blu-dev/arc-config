use std::{collections::HashMap, path::StripPrefixError};

use crate::{search, ToExternal, ToSmashArc};
use camino::{FromPathBufError, Utf8Path, Utf8PathBuf};
use hash40::label_map::LabelMap;
use smash_arc::{Hash40, LookupError, SearchLookup};
use thiserror::Error;

const INVALID: usize = 0xFF_FFFFusize;

#[derive(Error, Debug)]
pub enum GenerateError {
    #[error("The hash or index provided is for a file and not a folder")]
    InvalidFolder,

    #[error("An invalid path index was encountered")]
    InvalidPathIndex,

    #[error("Failed to find resource")]
    Lookup(#[from] LookupError),

    #[error("Failed to convert PathBuf to UTF-8")]
    ConversionError(#[from] FromPathBufError),

    #[error("Source folder does not exist")]
    MissingSourceFolder,

    #[error("Invalid root path provided")]
    InvalidRoot(#[from] StripPrefixError),

    #[error("Search error")]
    Search(#[from] search::SearchError),

    #[error("IO Error")]
    IO(#[from] std::io::Error),
}

enum SearchEntry {
    File(usize),
    Folder {
        path_index: usize,
        children: Vec<SearchEntry>,
    },
}

trait SearchEntryVecExt {
    fn flatten(self) -> Self;
}

impl SearchEntryVecExt for Vec<SearchEntry> {
    fn flatten(self) -> Self {
        let mut out_vec = vec![];
        for entry in self {
            match entry {
                SearchEntry::File(index) => out_vec.push(SearchEntry::File(index)),
                SearchEntry::Folder { children, .. } => out_vec.extend(children.flatten()),
            }
        }
        out_vec
    }
}

/// Performs a walk of the search
/// ### Arguments
/// * `folder` - The folder to search (searching "/" will search the root of the filesystem)
/// * `search` - The search section
/// * `depth` - An optional value to specify how deep the search should go. Passing `0` means no results at all, and passing `None` means to search until the bottom
///
/// ### Returns
/// * `Ok(children)` - A `Vec` of the child entries
/// * `Err` - A [`GenerateError`]
fn walk_search_section<H: ToSmashArc>(
    search: &impl SearchLookup,
    folder: H,
    depth: Option<usize>,
) -> Result<Vec<SearchEntry>, GenerateError> {
    // Begin by checking for the end of our recursive case, which is a 0-depth search
    // A zero depth search should result in no results period.
    if let Some(depth) = depth && depth == 0 {
        return Ok(vec![]);
    }

    let folder = folder.to_smash_arc();

    // Get our base folder, making sure that it is not for a file along the way

    let folder = if folder == Hash40::from("/") {
        // skip getting path since it doesn't exist
        search
            .get_folder_path_entry_from_hash(folder)
            .map_err(GenerateError::from)?
    } else {
        search
            .get_path_list_entry_from_hash(folder)
            .map_err(GenerateError::from)
            .and_then(|path| {
                if path.is_directory() {
                    search
                        .get_folder_path_entry_from_hash(path.path.hash40())
                        .map_err(GenerateError::from)
                } else {
                    Err(GenerateError::InvalidFolder)
                }
            })?
    };

    let mut current_child = folder.get_first_child_index();
    let mut children = vec![];

    // Get our arrays head of time so the code is readable
    let indices = search.get_path_list_indices();
    let paths = search.get_path_list();

    let next_depth = depth.map(|depth| depth - 1);

    while current_child != INVALID {
        let child_index = indices[current_child] as usize;

        if child_index == INVALID {
            return Err(GenerateError::InvalidPathIndex);
        }

        let child = &paths[child_index];

        if child.is_directory() {
            children.push(SearchEntry::Folder {
                path_index: child_index,
                children: walk_search_section(search, child.path.hash40(), next_depth)?,
            })
        } else {
            children.push(SearchEntry::File(child_index));
        }

        current_child = child.path.index() as usize;
    }

    Ok(children)
}

#[allow(unused)]
fn get_direct_child_from_parent_hash<H: ToSmashArc, H2: ToSmashArc>(
    search: &impl SearchLookup,
    parent: H,
    child: H2,
) -> Result<Option<usize>, GenerateError> {
    let parent = parent.to_smash_arc();
    let child = child.to_smash_arc();

    let path_index = search
        .get_path_list_index_from_hash(parent)
        .map_err(GenerateError::from)?;

    get_direct_child(search, path_index as usize, child)
}

fn get_direct_child<H: ToSmashArc>(
    search: &impl SearchLookup,
    parent: usize,
    child: H,
) -> Result<Option<usize>, GenerateError> {
    let child = child.to_smash_arc();

    // Get our base folder, making sure that it is not for a file along the way
    let folder = &search.get_path_list()[parent];

    let folder = if folder.is_directory() {
        search
            .get_folder_path_entry_from_hash(folder.path.hash40())
            .map_err(GenerateError::from)?
    } else {
        return Err(GenerateError::InvalidFolder);
    };

    let mut current_child = folder.get_first_child_index();

    // Get our arrays head of time so the code is readable
    let indices = search.get_path_list_indices();
    let paths = search.get_path_list();

    while current_child != INVALID {
        let child_index = indices[current_child] as usize;

        if child_index == INVALID {
            return Err(GenerateError::InvalidPathIndex);
        }

        let path = &paths[child_index];

        if path.file_name.hash40() == child {
            return Ok(Some(child_index));
        }

        current_child = path.path.index() as usize;
    }

    Ok(None)
}

fn compare_folders_impl(
    search: &impl SearchLookup,
    src: Hash40,
    dst: Hash40,
    parent: search::Folder,
) -> Result<HashMap<hash40::Hash40, search::File>, GenerateError> {
    // first ensure that the source directory exists. If it doesn't exist then we don't
    // know the intended behavior so return an error
    if search.get_path_list_entry_from_hash(src).is_err() {
        return Err(GenerateError::MissingSourceFolder);
    }
    // get the index of the destination path entry if it exists
    let dst_index = search
        .get_path_list_index_from_hash(dst)
        .ok()
        .map(|index| index as usize);
    // do a 1-depth shallow walk on the source folder
    let src_entries = walk_search_section(search, src, Some(1))?;
    let mut missing = HashMap::new();
    // iterate over each entry and check if the file exists
    for entry in src_entries {
        match entry {
            SearchEntry::File(index) => {
                // get the path entry
                let path_entry = &search.get_path_list()[index];
                // if a file with the same name exists in the destination directory then we just move on
                if let Some(index) = dst_index && get_direct_child(search, index, path_entry.file_name.hash40())?.is_some() {
                    continue;
                }
                // Otherwise, we are going to insert a new file into our list
                // of missing files
                let file_name = path_entry.file_name.hash40().to_external();
                let extension = path_entry.ext.hash40().to_external();
                missing.insert(
                    path_entry.path.hash40().to_external(),
                    search::File {
                        full_path: parent.full_path.join_path(file_name),
                        file_name,
                        parent: parent.clone(),
                        extension,
                    },
                );
            }
            SearchEntry::Folder { path_index, .. } => {
                // get the path entry
                let path_entry = &search.get_path_list()[path_index];
                // we don't care if it exists or not, since we are checking all of the files.
                // Folders are automatically inserted by arcropolis if they are missing.
                let dst_name = dst
                    .to_external()
                    .join_path(path_entry.file_name.hash40().to_external());
                // Create the next folder in the hierarchy so all of the children can reference their correct parent
                let next_folder = search::Folder {
                    full_path: dst_name,
                    name: Some(path_entry.file_name.hash40().to_external()),
                    parent: Some(Box::new(parent.clone())),
                };
                // Extend our missing files
                missing.extend(compare_folders_impl(
                    search,
                    path_entry.path.hash40(),
                    dst_name.to_smash_arc(),
                    next_folder,
                )?)
            }
        }
    }
    Ok(missing)
}

/// This method reports the difference in two folders in the search section. It's important to note
/// that this compares in the search section, so it's primary intended purpose is to generate a list of files that are present in one folder
/// but not in another, intending for generation of shared files.
///
/// An example use case of this would be comparing two fighter slots, where one is intended to be based on the other:
/// ```rs
/// let arc = ArcFile::open("D:/data.arc").unwrap();
/// let difference = compare_folders(
///     &arc,
///     "fighter/mario/model/body/c00",
///     "fighter/mario/model/body/c08"
/// );
/// ```
///
/// Note that since this operates on the search section, this should really only be handled during runtime checks. A similar
/// check can be done on an actual filesystem, however the implementations are different due to the requirement of actual paths instead of
/// hashes
///
/// ### Arguments
/// - `search` - A reference to an object that implements the search lookups
/// - `src` - The source folder to compare to
/// - `dst` - The destination folder to compare from
pub fn compare_folders(
    search: &impl SearchLookup,
    src: impl ToSmashArc,
    dst: impl ToSmashArc,
) -> Result<HashMap<hash40::Hash40, search::File>, GenerateError> {
    let src = src.to_smash_arc();
    let dst = dst.to_smash_arc();

    let folder = search::Folder {
        full_path: dst.to_external(),
        name: None,
        parent: None,
    };

    compare_folders_impl(search, src, dst, folder)
}

pub fn compare_folders_path(
    search: &impl SearchLookup,
    src: impl ToSmashArc,
    dst: &Utf8Path,
    root: &Utf8Path,
) -> Result<HashMap<hash40::Hash40, search::File>, GenerateError> {
    let src = src.to_smash_arc();

    // First ensure that the source folder exists, otherwise we cannot compare
    if search.get_path_list_entry_from_hash(src).is_err() {
        return Err(GenerateError::MissingSourceFolder);
    }

    let map = hash40::Hash40::label_map();
    let mut labels = map.lock().unwrap();
    for component in dst.strip_prefix(root)?.components() {
        labels.add_labels(vec![component.to_string()]);
    }
    drop(labels);
    drop(map);

    // check if the destination exists
    if !dst.exists() {
        let missing_folder_name = dst.strip_prefix(root)?.as_str().to_external();

        let missing_folder = search::Folder {
            full_path: missing_folder_name,
            name: Some(dst.file_name().unwrap().to_external()),
            parent: Some(Box::new(search::Folder::from_path(
                dst.parent().unwrap().strip_prefix(root)?,
            )?)),
        };

        return compare_folders_impl(
            search,
            src,
            missing_folder_name.to_smash_arc(),
            missing_folder,
        );
    }

    // do a shallow walk on the source path
    let src_entries = walk_search_section(search, src, Some(1))?;

    // get the entries of the destination folder and create a hashmap of it's entries.
    // unlike the search-only method, we also care about directories because we need to get the path
    // to continue traversing if it exists. If it does not exist, then we can effectively just call the
    // `compare_folders` method
    let dst_entries = {
        let entries = dst.read_dir_utf8()?;
        let mut entry_hashes = HashMap::new();
        for entry in entries {
            let entry = entry?;

            let unix_style: Utf8PathBuf = entry.path().as_str().replace('\\', "/").into();

            entry_hashes.insert(entry.file_name().to_smash_arc(), unix_style);

            let map = hash40::Hash40::label_map();
            let mut labels = map.lock().unwrap();
            labels.add_labels(vec![entry.file_name().to_string()]);
        }
        entry_hashes
    };

    let mut missing = HashMap::new();

    for entry in src_entries {
        match entry {
            SearchEntry::File(index) => {
                let path = &search.get_path_list()[index];

                if dst_entries.contains_key(&path.file_name.hash40()) {
                    continue;
                }

                let file_name = path.file_name.hash40().to_external();

                missing.insert(
                    path.path.hash40().to_external(),
                    search::File {
                        full_path: dst.as_str().to_external().join_path(file_name),
                        file_name,
                        parent: search::Folder::from_path(dst.strip_prefix(root)?)?,
                        extension: path.ext.hash40().to_external(),
                    },
                );
            }
            SearchEntry::Folder { path_index, .. } => {
                let path = &search.get_path_list()[path_index];

                if let Some(child_path) = dst_entries.get(&path.file_name.hash40()) {
                    if child_path.is_file() {
                        return Err(GenerateError::InvalidFolder);
                    }

                    missing.extend(compare_folders_path(
                        search,
                        path.path.hash40(),
                        child_path,
                        root,
                    )?);
                } else {
                    let missing_folder_name = dst
                        .strip_prefix(root)?
                        .as_str()
                        .to_external()
                        .join_path(path.file_name.hash40().to_external());

                    let missing_folder = search::Folder {
                        full_path: missing_folder_name,
                        name: Some(path.file_name.hash40().to_external()),
                        parent: Some(Box::new(search::Folder::from_path(
                            dst.strip_prefix(root)?,
                        )?)),
                    };

                    missing.extend(compare_folders_impl(
                        search,
                        path.path.hash40(),
                        missing_folder_name.to_smash_arc(),
                        missing_folder,
                    )?)
                }
            }
        }
    }

    Ok(missing)
}

/// Updates the label map with all possible derived hashes from the search section.
///
/// For example, if the label map contains the label-hash pair for `stage/poke_stadium2/normal/param/xstadium_02.lvd` but not for `param`,
/// this method will reverse the directory hierarchy for `stage/poke_stadium2/normal/param/xstadium_02.lvd` and ensure that all hashes
/// there are included in the label map.
///
/// This is important for the [`search`] module, as it allows folder paths to construct new hashes based on only file names
pub fn fill_label_map_from_search(
    search: &impl SearchLookup,
    label_map: &mut LabelMap,
) -> Result<(), GenerateError> {
    fn build_new_path(
        search: &impl SearchLookup,
        file_index: usize,
        label_map: &LabelMap,
    ) -> Option<String> {
        let path = &search.get_path_list()[file_index];

        // cover degenerate case
        if let Some(label) = label_map.label_of(path.path.hash40().to_external()) {
            return Some(label);
        }

        let name = label_map.label_of(path.file_name.hash40().to_external())?;
        let parent = if let Some(parent) = label_map.label_of(path.parent.hash40().to_external()) {
            parent
        } else {
            search
                .get_path_list_index_from_hash(path.parent.hash40())
                .ok()
                .and_then(|index| build_new_path(search, index as usize, label_map))?
        };

        Some(format!("{}/{}", parent, name))
    }

    let all_files = walk_search_section(search, "/", None).map(SearchEntryVecExt::flatten)?;

    let paths = search.get_path_list();

    for file in all_files {
        let SearchEntry::File(index) = file else {
            unreachable!()
        };

        let path = &paths[index];

        // check if the label exists for this string
        if let Some(label) = label_map.label_of(path.path.hash40().to_external()) {
            // if it does, we are going to convert it into a path and continually insert all of the components
            // into the label map
            let label_path = Utf8PathBuf::from(label);
            for component in label_path.components() {
                label_map.add_labels(vec![component.to_string()]);
            }
        }
        // the label does not exist, which means we are going to try recursively constructing the new label passed on the search section hierarchy
        else if let Some(label) = build_new_path(search, index, label_map) {
            label_map.add_labels(vec![label])
        }
    }

    Ok(())
}
