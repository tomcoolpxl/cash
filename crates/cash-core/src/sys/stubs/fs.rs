//! Filesystem utilities (stubs).

pub(crate) trait MetadataExt {
    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

impl MetadataExt for std::fs::Metadata {}
