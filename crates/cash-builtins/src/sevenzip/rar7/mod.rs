//! RAR as 7-Zip 26.03's RAR handlers list and read it (`Rar5Handler.cpp`,
//! `RarHandler.cpp`): the volumes of a set found by name (`RarVol.h`), their headers
//! read into items, and each property as 7-Zip shows it. The data is decoded by
//! cash-archive's `rar`.

mod data;
mod rar4;
mod rar5;
mod volname;

use std::io;
use std::path::{Path, PathBuf};

use super::archive::{Data, Item, OpenFailure, Prop};

/// What opening a RAR archive found.
pub(super) struct Opening {
    pub(super) physical_size: u64,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) item_props: Vec<Prop>,
    pub(super) items: Vec<Item>,
    pub(super) error_flags: Vec<&'static str>,
    pub(super) warning_flags: Vec<&'static str>,
    /// `kpidError`: a volume the set needed that was not there.
    pub(super) error_message: Option<String>,
    /// Every volume read, the one named first among them.
    pub(super) volumes: Vec<PathBuf>,
    pub(super) rar: Rar,
}

/// A RAR archive, open, for reading its items' data with the password it was opened
/// with or was given after.
pub(super) struct Rar {
    format: Format,
    password: Option<String>,
}

enum Format {
    Four(Box<rar4::Rar4>),
    Five(Box<rar5::Rar5>),
}

impl Rar {
    pub(super) fn set_password(&mut self, password: &str) {
        self.password = Some(password.to_owned());
    }

    /// Whether extracting the item asks for a password.
    pub(super) fn needs_password(&self, index: usize) -> bool {
        match &self.format {
            Format::Five(rar) => rar.needs_password(index),
            Format::Four(rar) => rar.needs_password(index),
        }
    }

    /// Hands the items `wanted` names their data, in the archive's order, decoding the
    /// solid items before them that their streams need.
    pub(super) fn extract<E: From<io::Error>>(
        &self,
        items: &[Item],
        wanted: &dyn Fn(usize) -> bool,
        each: impl FnMut(usize, &mut dyn Data) -> Result<bool, E>,
    ) -> Result<(), E> {
        let password = self.password.as_deref();
        let encrypted = |index: usize| items.get(index).is_some_and(|i| i.encrypted);
        match &self.format {
            Format::Five(rar) => {
                let steps = data::plan5(rar, wanted);
                data::run(
                    &steps,
                    &encrypted,
                    |steps, tx| data::work5(rar, password, steps, tx),
                    each,
                )
            }
            Format::Four(rar) => {
                let steps = data::plan4(rar, wanted);
                data::run(
                    &steps,
                    &encrypted,
                    |steps, tx| data::work4(rar, password, steps, tx),
                    each,
                )
            }
        }
    }
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
        warning_flags: Vec::new(),
        error_message: rar
            .missing_volume
            .as_ref()
            .map(|name| format!("Missing volume : {name}")),
        volumes: rar.volumes.clone(),
        rar: Rar {
            format: Format::Five(Box::new(rar)),
            password: password.map(str::to_owned),
        },
    })
}

/// Opens a RAR 1.5 to 4 archive and the volumes after it.
pub(super) fn open4(
    path: &Path,
    password: Option<&str>,
    zone: &cash_core::timefmt::Zone,
) -> Result<Opening, OpenFailure> {
    let rar = match rar4::Rar4::open(path, password).map_err(OpenFailure::Io)? {
        Ok(rar) => rar,
        Err(rar4::Failure::PasswordNeeded) => return Err(OpenFailure::PasswordNeeded),
        Err(rar4::Failure::WrongPassword) => return Err(OpenFailure::WrongPassword),
        Err(rar4::Failure::NotArchive) => {
            return Err(OpenFailure::NotArchive {
                tried: Some(super::archive::Kind::Rar),
                flags: vec!["Is not archive"],
                in_split: false,
            });
        }
    };
    Ok(Opening {
        physical_size: rar.info.phy_size(),
        props: rar.archive_props(),
        item_props: rar4::ITEM_PROPS.to_vec(),
        items: rar.listed(zone),
        error_flags: rar.error_flags(),
        warning_flags: rar.warning_flags(),
        error_message: rar
            .missing_volume
            .as_ref()
            .map(|name| format!("Missing volume : {name}")),
        volumes: rar.volumes.clone(),
        rar: Rar {
            format: Format::Four(Box::new(rar)),
            password: password.map(str::to_owned),
        },
    })
}
