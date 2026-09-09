use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use beeriokartbracket::{
    PersistenceError, Tournament, deserialize_tournament, serialize_tournament,
};

pub(crate) struct FileSession {
    path: PathBuf,
    name: String,
    save_error: Option<String>,
}

pub(crate) struct OpenedTournament {
    pub(crate) session: FileSession,
    pub(crate) tournament: Tournament,
    pub(crate) recovered_backup: bool,
}

impl FileSession {
    pub(crate) fn create(
        path: PathBuf,
        name: String,
        tournament: &Tournament,
    ) -> Result<Self, FileError> {
        let mut session = Self {
            path,
            name,
            save_error: None,
        };
        session.save(tournament)?;
        Ok(session)
    }

    pub(crate) fn open(path: PathBuf) -> Result<OpenedTournament, FileError> {
        match read_tournament(&path) {
            Ok((name, tournament)) => Ok(OpenedTournament {
                session: Self {
                    path,
                    name,
                    save_error: None,
                },
                tournament,
                recovered_backup: false,
            }),
            Err(primary_error) => {
                let backup = backup_path(&path);
                let (name, tournament) =
                    read_tournament(&backup).map_err(|backup_error| FileError::RecoveryFailed {
                        primary: primary_error.to_string(),
                        backup: backup_error.to_string(),
                    })?;
                fs::copy(&backup, &path)?;
                Ok(OpenedTournament {
                    session: Self {
                        path,
                        name,
                        save_error: None,
                    },
                    tournament,
                    recovered_backup: true,
                })
            }
        }
    }

    pub(crate) fn save(&mut self, tournament: &Tournament) -> Result<(), FileError> {
        let result = write_tournament(&self.path, &self.name, tournament);
        self.save_error = result.as_ref().err().map(ToString::to_string);
        result
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub(crate) fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }
}

#[derive(Debug)]
pub(crate) enum FileError {
    Io(io::Error),
    Tournament(PersistenceError),
    RecoveryFailed { primary: String, backup: String },
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "file operation failed: {error}"),
            Self::Tournament(error) => error.fmt(formatter),
            Self::RecoveryFailed { primary, backup } => write!(
                formatter,
                "the tournament and its backup could not be opened\nPrimary: {primary}\nBackup: {backup}"
            ),
        }
    }
}

impl Error for FileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Tournament(error) => Some(error),
            Self::RecoveryFailed { .. } => None,
        }
    }
}

impl From<io::Error> for FileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<PersistenceError> for FileError {
    fn from(error: PersistenceError) -> Self {
        Self::Tournament(error)
    }
}

pub(crate) fn choose_new_path(suggested_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Beerio Kart tournament", &["bk.json"])
        .set_file_name(format!("{suggested_name}.bk.json"))
        .save_file()
}

pub(crate) fn choose_open_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Beerio Kart tournament", &["bk.json"])
        .pick_file()
}

fn write_tournament(path: &Path, name: &str, tournament: &Tournament) -> Result<(), FileError> {
    let json = serialize_tournament(name, tournament)?;

    if read_tournament(path).is_ok() {
        fs::copy(path, backup_path(path))?;
    }

    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    Ok(())
}

fn read_tournament(path: &Path) -> Result<(String, Tournament), FileError> {
    let json = fs::read_to_string(path)?;
    Ok(deserialize_tournament(&json)?)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    backup.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_keeps_one_previous_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.bk.json");
        let mut tournament = Tournament::default();
        let mut session =
            FileSession::create(path.clone(), "Test Cup".to_owned(), &tournament).unwrap();

        tournament.add_participant("Mario").unwrap();
        session.save(&tournament).unwrap();

        let (_, primary) = read_tournament(&path).unwrap();
        let (_, backup) = read_tournament(&backup_path(&path)).unwrap();
        assert_eq!(participant_count(&primary), 1);
        assert_eq!(participant_count(&backup), 0);
    }

    #[test]
    fn corrupt_primary_recovers_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.bk.json");
        let mut tournament = Tournament::default();
        let mut session =
            FileSession::create(path.clone(), "Test Cup".to_owned(), &tournament).unwrap();
        tournament.add_participant("Mario").unwrap();
        session.save(&tournament).unwrap();
        fs::write(&path, "{").unwrap();

        let opened = FileSession::open(path.clone()).unwrap();

        assert!(opened.recovered_backup);
        assert_eq!(participant_count(&opened.tournament), 0);
        assert!(read_tournament(&path).is_ok());
    }

    #[test]
    fn failed_save_is_retained_by_the_session() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = FileSession {
            path: directory.path().to_owned(),
            name: "Test Cup".to_owned(),
            save_error: None,
        };

        assert!(session.save(&Tournament::default()).is_err());
        assert!(session.save_error().is_some());
    }

    fn participant_count(tournament: &Tournament) -> usize {
        let beeriokartbracket::TournamentView::Registration(registration) = tournament.view()
        else {
            panic!("test tournament should be in registration");
        };
        registration.participants.len()
    }
}
