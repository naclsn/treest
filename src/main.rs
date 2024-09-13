use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

mod tree;

use crate::tree::{Provider, Tree};

#[derive(Debug)]
struct FsProvider {}
#[derive(Debug)]
enum FsNodeData {
    Dir,
    File,
}

impl Provider for FsProvider {
    type Data = FsNodeData;
    type Fragment = OsString;

    fn provide(path: Vec<&Self::Fragment>) -> Vec<(Self::Data, Self::Fragment)> {
        let Ok(dir) = fs::read_dir(path.into_iter().collect::<PathBuf>()) else {
            return Vec::new();
        };
        dir.filter_map(|d| {
            d.ok().and_then(|d| {
                d.metadata().ok().map(|m| {
                    (
                        if m.is_dir() {
                            FsNodeData::Dir
                        //} else if m.is_symlink() {
                        //    FsNodeData::Link
                        } else {
                            FsNodeData::File
                        },
                        d.file_name(),
                    )
                })
            })
        })
        .collect()
    }
}

fn main() {
    let app = Tree::new(FsProvider {}, (FsNodeData::Dir, ".".into()));
    println!("{app:#?}");
}
