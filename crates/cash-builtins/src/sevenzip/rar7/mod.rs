//! RAR as 7-Zip 26.03's RAR handlers list and read it (`Rar5Handler.cpp`,
//! `RarHandler.cpp`): the volumes of a set found by name (`RarVol.h`), their headers
//! read into items, and each property as 7-Zip shows it. The data is decoded by
//! cash-archive's `rar`.

mod rar5;
mod volname;

use std::io;
use std::path::{Path, PathBuf};

use super::archive::{Item, OpenFailure, Prop};

/// What opening a RAR archive found.
pub(super) struct Opening {
    pub(super) physical_size: u64,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) item_props: Vec<Prop>,
    pub(super) items: Vec<Item>,
    pub(super) error_flags: Vec<&'static str>,
    /// `kpidError`: a volume the set needed that was not there.
    pub(super) error_message: Option<String>,
    /// Every volume read, the one named first among them.
    pub(super) volumes: Vec<PathBuf>,
    pub(super) rar: Rar,
}

/// A RAR archive, open, for reading its items' data.
pub(super) enum Rar {
    Five(#[expect(dead_code, reason = "read by extraction, the next step")] Box<rar5::Rar5>),
}

/// Opens a RAR 5 archive and the volumes after it.
pub(super) fn open5(
    path: &Path,
    password: Option<&str>,
    zone: &cash_core::timefmt::Zone,
) -> Result<Opening, OpenFailure> {
    let rar = match rar5::Rar5::open(path, password).map_err(OpenFailure::Io)? {
        Ok(rar) => rar,
        Err(rar5::Failure::PasswordNeeded) => return Err(OpenFailure::PasswordNeeded),
        Err(rar5::Failure::WrongPassword) => return Err(OpenFailure::WrongPassword),
        Err(rar5::Failure::UnexpectedEnd) => {
            return Err(OpenFailure::NotArchive {
                tried: Some(super::archive::Kind::Rar5),
                flags: vec!["Unexpected end of archive"],
                in_split: false,
            });
        }
        Err(rar5::Failure::NotArchive) => {
            return Err(OpenFailure::NotArchive {
                tried: Some(super::archive::Kind::Rar5),
                flags: vec!["Is not archive"],
                in_split: false,
            });
        }
    };
    let physical_size = rar.infos.first().map_or(0, rar5::ArcInfo::phy_size);
    Ok(Opening {
        physical_size,
        props: rar.archive_props(zone),
        item_props: rar5::ITEM_PROPS.to_vec(),
        items: rar.listed(),
        error_flags: rar.error_flags(),
        error_message: rar
            .missing_volume
            .as_ref()
            .map(|name| format!("Missing volume : {name}")),
        volumes: rar.volumes.clone(),
        rar: Rar::Five(Box::new(rar)),
    })
}

/// The data of RAR items is not read yet.
pub(super) fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "RAR data is not read yet")
}
